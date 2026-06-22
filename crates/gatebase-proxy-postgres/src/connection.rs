use crate::audit::QueryContext;
use crate::protocol::{
    is_clean_disconnect, parse_startup, read_message, read_startup, request_password,
    write_auth_ok, write_backend_key_data, write_error, write_parameter_status, write_ready,
};
use crate::query::handle_query;
use crate::upstream::upstream_config;
use anyhow::{Context, Result};
use chrono::Utc;
use gatebase_audit::AuditSink;
use gatebase_config::{PolicyConfig, TargetConfig};
use gatebase_core::{ActiveConnection, Session};
use gatebase_session::{SessionIssuer, SessionStore};
use std::sync::Arc;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::time::{interval, sleep, Duration};
use uuid::Uuid;

pub(crate) async fn handle_connection(
    stream: tokio::net::TcpStream,
    target: TargetConfig,
    policy: PolicyConfig,
    sinks: Vec<Arc<dyn AuditSink>>,
    store: SessionStore,
    issuer: SessionIssuer,
    fail_closed: bool,
) -> Result<()> {
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
                    result = handle_query(&statement, &upstream, &mut writer, &query_context) => result?,
                    reason = wait_for_session_end(&store, &session) => {
                        write_error(&mut writer, "57P01", session_disconnect_message(reason?)).await?;
                        return Ok(());
                    }
                }
                write_ready(&mut writer).await?;
            }
            b'X' => return Ok(()),
            _ => {
                write_error(
                    &mut writer,
                    "0A000",
                    "Gatebase currently supports only Postgres simple Query messages",
                )
                .await?;
                write_ready(&mut writer).await?;
            }
        }
    }
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
