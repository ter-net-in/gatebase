use crate::audit::QueryContext;
use crate::protocol::{
    is_clean_disconnect, parse_startup, read_message, read_startup, request_password,
    write_auth_ok, write_backend_key_data, write_error, write_parameter_status, write_ready,
};
use crate::query::{describe_query, handle_extended_query, handle_query};
use crate::rollback::RollbackContext;
use crate::upstream::upstream_config;
use anyhow::{Context, Result};
use chrono::Utc;
use gatebase_audit::{AuditSink, RollbackSink};
use gatebase_config::{PolicyConfig, RollbackConfig, TargetConfig};
use gatebase_core::{ActiveConnection, Session};
use gatebase_session::{SessionIssuer, SessionStore};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::time::{interval, sleep, Duration};
use uuid::Uuid;

pub(crate) struct ConnectionParams {
    pub target: TargetConfig,
    pub policy: PolicyConfig,
    pub sinks: Vec<Arc<dyn AuditSink>>,
    pub rollback: RollbackConfig,
    pub rollback_sinks: Vec<Arc<dyn RollbackSink>>,
    pub store: SessionStore,
    pub issuer: SessionIssuer,
    pub fail_closed: bool,
}

pub(crate) async fn handle_connection(
    stream: tokio::net::TcpStream,
    params: ConnectionParams,
) -> Result<()> {
    let ConnectionParams {
        target,
        policy,
        sinks,
        rollback,
        rollback_sinks,
        store,
        issuer,
        fail_closed,
    } = params;
    let peer = stream.peer_addr()?;
    let (mut reader, mut writer) = stream.into_split();
    let startup = read_startup(&mut reader, &mut writer).await?;
    let params = parse_startup(&startup)?;
    let token = request_password(&mut reader, &mut writer).await?;
    let verified = match issuer.verify_active(&store, &token, &target.name).await {
        Ok(verified) => verified,
        Err(error) => {
            write_error(&mut writer, "28P01", &error.to_string()).await?;
            return Ok(());
        }
    };

    write_auth_ok(&mut writer).await?;
    write_parameter_status(&mut writer, "server_version", "16.0").await?;
    write_parameter_status(&mut writer, "client_encoding", "UTF8").await?;
    write_parameter_status(&mut writer, "DateStyle", "ISO, MDY").await?;
    write_backend_key_data(&mut writer).await?;
    write_ready(&mut writer).await?;

    let (upstream, connection) =
        tokio_postgres::connect(&upstream_config(&target)?, tokio_postgres::NoTls)
            .await
            .context("failed to connect to upstream Postgres")?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            tracing::warn!(%error, "upstream Postgres connection failed");
        }
    });

    tracing::info!(
        user = params.get("user").map(String::as_str).unwrap_or(""),
        database = params.get("database").map(String::as_str).unwrap_or(""),
        target = %target.name,
        session_id = %verified.token_session_id,
        "postgres session accepted"
    );

    let active_connection = ActiveConnection {
        id: format!("conn_{}", Uuid::new_v4().simple()),
        session_id: verified.session.id.clone(),
        target: target.name.clone(),
        client_addr: peer.to_string(),
        connected_at: Utc::now(),
        disconnected_at: None,
    };
    store.create_active_connection(&active_connection).await?;
    let result = proxy_loop(ProxyLoop {
        reader,
        writer,
        upstream,
        target,
        policy,
        sinks,
        rollback,
        rollback_sinks,
        store: store.clone(),
        session: verified.session,
        fail_closed,
    })
    .await;
    store.close_active_connection(&active_connection.id).await?;
    result
}

struct ProxyLoop {
    reader: OwnedReadHalf,
    writer: OwnedWriteHalf,
    upstream: tokio_postgres::Client,
    target: TargetConfig,
    policy: PolicyConfig,
    sinks: Vec<Arc<dyn AuditSink>>,
    rollback: RollbackConfig,
    rollback_sinks: Vec<Arc<dyn RollbackSink>>,
    store: SessionStore,
    session: Session,
    fail_closed: bool,
}

async fn proxy_loop(loop_state: ProxyLoop) -> Result<()> {
    let ProxyLoop {
        mut reader,
        mut writer,
        upstream,
        target,
        policy,
        sinks,
        rollback,
        rollback_sinks,
        store,
        session,
        fail_closed,
    } = loop_state;
    let query_context = QueryContext {
        target: &target,
        policy: &policy,
        sinks: &sinks,
        session: &session,
        fail_closed,
    };
    let rollback_context = RollbackContext {
        config: &rollback,
        sinks: &rollback_sinks,
        session: &session,
        target: &target,
        fail_closed,
    };
    let mut extended = ExtendedQueryState::default();

    loop {
        let message = tokio::select! {
            reason = wait_for_session_end(&store, &session) => {
                write_error(&mut writer, "57P01", session_disconnect_message(reason?)).await?;
                return Ok(());
            }
            message = read_message(&mut reader) => match message {
                Ok(message) => message,
                Err(error) if is_clean_disconnect(&error) => return Ok(()),
                Err(error) => return Err(error),
            },
        };
        match message.tag {
            b'Q' => {
                let statement = crate::protocol::cstring(&message.body)
                    .context("invalid Query message")?
                    .to_owned();
                tokio::select! {
                    result = handle_query(&statement, &upstream, &mut writer, &query_context, &rollback_context) => result?,
                    reason = wait_for_session_end(&store, &session) => {
                        write_error(&mut writer, "57P01", session_disconnect_message(reason?)).await?;
                        return Ok(());
                    }
                }
                write_ready(&mut writer).await?;
            }
            b'P' => {
                let (name, statement) = crate::protocol::parse_statement_message(&message.body)
                    .context("invalid Parse message")?;
                extended
                    .statements
                    .insert(name.to_owned(), statement.to_owned());
                crate::protocol::write_parse_complete(&mut writer).await?;
            }
            b'B' => {
                let (portal, statement, parameter_count) =
                    crate::protocol::parse_bind_message(&message.body)
                        .context("invalid Bind message")?;
                if parameter_count != 0 {
                    write_error(
                        &mut writer,
                        "0A000",
                        "Gatebase does not support Postgres extended protocol parameters yet",
                    )
                    .await?;
                    continue;
                }
                let Some(statement_sql) = extended.statements.get(statement).cloned() else {
                    write_error(&mut writer, "26000", "unknown prepared statement").await?;
                    continue;
                };
                extended.portals.insert(portal.to_owned(), statement_sql);
                extended.described_portals.remove(portal);
                if extended.described_statements.contains(statement) {
                    extended.described_portals.insert(portal.to_owned());
                }
                crate::protocol::write_bind_complete(&mut writer).await?;
            }
            b'D' => {
                let (describe_type, name) = crate::protocol::parse_describe_message(&message.body)
                    .context("invalid Describe message")?;
                match describe_type {
                    b'P' => {
                        let Some(statement) = extended.portals.get(name) else {
                            write_error(&mut writer, "34000", "unknown portal").await?;
                            continue;
                        };
                        if describe_query(statement, &upstream, &mut writer, &query_context).await?
                        {
                            extended.described_portals.insert(name.to_owned());
                        }
                    }
                    b'S' => {
                        let Some(statement) = extended.statements.get(name) else {
                            write_error(&mut writer, "26000", "unknown prepared statement").await?;
                            continue;
                        };
                        crate::protocol::write_empty_parameter_description(&mut writer).await?;
                        if describe_query(statement, &upstream, &mut writer, &query_context).await?
                        {
                            extended.described_statements.insert(name.to_owned());
                        }
                    }
                    _ => {
                        write_error(&mut writer, "0A000", "unsupported Describe target").await?;
                        continue;
                    }
                }
            }
            b'E' => {
                let portal = crate::protocol::parse_execute_message(&message.body)
                    .context("invalid Execute message")?;
                let Some(statement) = extended.portals.get(portal).cloned() else {
                    write_error(&mut writer, "34000", "unknown portal").await?;
                    continue;
                };
                let emit_row_description = !extended.described_portals.remove(portal);
                tokio::select! {
                    result = handle_extended_query(&statement, &upstream, &mut writer, &query_context, &rollback_context, emit_row_description) => result?,
                    reason = wait_for_session_end(&store, &session) => {
                        write_error(&mut writer, "57P01", session_disconnect_message(reason?)).await?;
                        return Ok(());
                    }
                }
            }
            b'C' => {
                let (close_type, name) = crate::protocol::parse_close_message(&message.body)
                    .context("invalid Close message")?;
                match close_type {
                    b'S' => {
                        extended.statements.remove(name);
                        extended.described_statements.remove(name);
                    }
                    b'P' => {
                        extended.portals.remove(name);
                        extended.described_portals.remove(name);
                    }
                    _ => {
                        write_error(&mut writer, "0A000", "unsupported Close target").await?;
                        continue;
                    }
                }
                crate::protocol::write_close_complete(&mut writer).await?;
            }
            b'H' => {}
            b'S' => {
                write_ready(&mut writer).await?;
            }
            b'X' => return Ok(()),
            _ => {
                write_error(
                    &mut writer,
                    "0A000",
                    "Gatebase does not support this Postgres frontend message yet",
                )
                .await?;
            }
        }
    }
}

#[derive(Default)]
struct ExtendedQueryState {
    statements: HashMap<String, String>,
    portals: HashMap<String, String>,
    described_statements: HashSet<String>,
    described_portals: HashSet<String>,
}

#[derive(Debug, Clone, Copy)]
enum SessionDisconnectReason {
    Expired,
    Revoked,
}

fn session_disconnect_message(reason: SessionDisconnectReason) -> &'static str {
    match reason {
        SessionDisconnectReason::Expired => "Gatebase session expired",
        SessionDisconnectReason::Revoked => "Gatebase session revoked",
    }
}

async fn wait_for_session_end(
    store: &SessionStore,
    session: &Session,
) -> Result<SessionDisconnectReason> {
    let mut expires = Box::pin(sleep(duration_until(session.expires_at)));
    let mut revocation_check = interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            () = &mut expires => return Ok(SessionDisconnectReason::Expired),
            _ = revocation_check.tick() => {
                let Some(current) = store.get(&session.id).await? else {
                    return Ok(SessionDisconnectReason::Revoked);
                };
                if current.revoked_at.is_some() {
                    return Ok(SessionDisconnectReason::Revoked);
                }
                if current.expires_at <= Utc::now() {
                    return Ok(SessionDisconnectReason::Expired);
                }
            }
        }
    }
}

fn duration_until(deadline: chrono::DateTime<Utc>) -> Duration {
    (deadline - Utc::now()).to_std().unwrap_or(Duration::ZERO)
}
