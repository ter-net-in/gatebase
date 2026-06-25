use crate::audit::{write_audit, QueryContext};
use crate::protocol::{
    handshake, read_packet, write_err, write_ok, write_result_set, COM_PING, COM_QUERY, COM_QUIT,
};
use crate::rollback::{capture_rollback_artifact, RollbackContext};
use crate::upstream::upstream_opts;
use anyhow::Result;
use chrono::Utc;
use gatebase_audit::{AuditSink, RollbackSink};
use gatebase_config::{PolicyConfig, RollbackConfig, TargetConfig};
use gatebase_core::{ActiveConnection, Decision, Session};
use gatebase_policy::decide;
use gatebase_session::{SessionIssuer, SessionStore};
use mysql_async::prelude::Queryable;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::time::{interval, sleep, timeout, Duration};
use uuid::Uuid;

const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

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
    mut stream: TcpStream,
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
    let login = handshake(&mut stream).await?;
    let verified = match issuer
        .verify_active(&store, &login.token, &target.name)
        .await
    {
        Ok(verified) => verified,
        Err(error) => {
            write_err(&mut stream, 2, 1045, &error.to_string()).await?;
            return Ok(());
        }
    };
    write_ok(&mut stream, login.ok_sequence, 0).await?;

    let mut upstream = mysql_async::Conn::new(upstream_opts(&target)?).await?;
    tracing::info!(
        user = %login.username,
        database = login.database.as_deref().unwrap_or(""),
        target = %target.name,
        session_id = %verified.token_session_id,
        "mysql session accepted"
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
    let result = command_loop(CommandLoop {
        stream,
        upstream: &mut upstream,
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
    let _ = upstream.disconnect().await;
    store.close_active_connection(&active_connection.id).await?;
    result
}

struct CommandLoop<'a> {
    stream: TcpStream,
    upstream: &'a mut mysql_async::Conn,
    target: TargetConfig,
    policy: PolicyConfig,
    sinks: Vec<Arc<dyn AuditSink>>,
    rollback: RollbackConfig,
    rollback_sinks: Vec<Arc<dyn RollbackSink>>,
    store: SessionStore,
    session: Session,
    fail_closed: bool,
}

async fn command_loop(loop_state: CommandLoop<'_>) -> Result<()> {
    let CommandLoop {
        mut stream,
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

    loop {
        let packet = tokio::select! {
            reason = wait_for_session_end(&store, &session) => {
                write_err(&mut stream, 1, 1045, session_disconnect_message(reason?)).await?;
                return Ok(());
            }
            packet = read_packet(&mut stream) => packet?,
        };

        let Some((&command, body)) = packet.payload.split_first() else {
            write_err(
                &mut stream,
                packet.sequence.wrapping_add(1),
                1064,
                "empty command",
            )
            .await?;
            continue;
        };
        match command {
            COM_QUIT => return Ok(()),
            COM_PING => write_ok(&mut stream, packet.sequence.wrapping_add(1), 0).await?,
            COM_QUERY => {
                let statement = String::from_utf8_lossy(body).to_string();
                let response_sequence = packet.sequence.wrapping_add(1);
                tokio::select! {
                    result = handle_query(&mut stream, upstream, response_sequence, &statement, &query_context, &rollback_context) => result?,
                    reason = wait_for_session_end(&store, &session) => {
                        write_err(&mut stream, 1, 1045, session_disconnect_message(reason?)).await?;
                        return Ok(());
                    }
                }
            }
            _ => {
                write_err(
                    &mut stream,
                    1,
                    1047,
                    "Gatebase currently supports only MySQL text query commands",
                )
                .await?;
            }
        }
    }
}

async fn handle_query(
    stream: &mut TcpStream,
    upstream: &mut mysql_async::Conn,
    response_sequence: u8,
    statement: &str,
    context: &QueryContext<'_>,
    rollback: &RollbackContext<'_>,
) -> Result<()> {
    let policy_decision = decide(statement, context.policy);
    if policy_decision.decision == Decision::Blocked {
        write_audit(
            context,
            statement,
            Decision::Blocked,
            None,
            policy_decision.reason.clone(),
            None,
        )
        .await?;
        write_err(
            stream,
            response_sequence,
            1142,
            policy_decision
                .reason
                .as_deref()
                .unwrap_or("statement blocked by Gatebase policy"),
        )
        .await?;
        return Ok(());
    }

    let rollback_artifact_id = capture_rollback_artifact(statement, upstream, rollback).await?;
    let enforce_row_limit =
        is_row_limited_mutation(statement) && context.policy.max_rows_changed.is_some();
    if enforce_row_limit {
        timeout(QUERY_TIMEOUT, upstream.query_drop("START TRANSACTION")).await??;
    }

    match timeout(QUERY_TIMEOUT, upstream.query_iter(statement)).await {
        Ok(Ok(mut result)) => {
            let columns = result.columns();
            if let Some(columns) = columns.filter(|columns| !columns.is_empty()) {
                let mut rows = Vec::new();
                while let Some(row) = result.next().await? {
                    rows.push(row);
                }
                let rows_affected = result.affected_rows() as i64;
                result.drop_result().await?;
                write_result_set(stream, response_sequence, columns, rows).await?;
                write_audit(
                    context,
                    statement,
                    Decision::Allowed,
                    Some(rows_affected),
                    None,
                    rollback_artifact_id,
                )
                .await?;
            } else {
                let affected_rows = result.affected_rows();
                result.drop_result().await?;
                if let Some(limit) = context.policy.max_rows_changed {
                    if enforce_row_limit && affected_rows > limit {
                        timeout(QUERY_TIMEOUT, upstream.query_drop("ROLLBACK")).await??;
                        let message =
                            format!("statement changed more than max_rows_changed {limit}");
                        write_audit(
                            context,
                            statement,
                            Decision::Blocked,
                            Some(affected_rows as i64),
                            Some(message.clone()),
                            rollback_artifact_id,
                        )
                        .await?;
                        write_err(stream, response_sequence, 1142, &message).await?;
                        return Ok(());
                    }
                }
                if enforce_row_limit {
                    timeout(QUERY_TIMEOUT, upstream.query_drop("COMMIT")).await??;
                }
                write_ok(stream, response_sequence, affected_rows).await?;
                write_audit(
                    context,
                    statement,
                    Decision::Allowed,
                    Some(affected_rows as i64),
                    None,
                    rollback_artifact_id,
                )
                .await?;
            }
        }
        Ok(Err(error)) => {
            if enforce_row_limit {
                let _ = timeout(QUERY_TIMEOUT, upstream.query_drop("ROLLBACK")).await;
            }
            let message = error.to_string();
            write_audit(
                context,
                statement,
                Decision::Allowed,
                None,
                Some(message.clone()),
                rollback_artifact_id,
            )
            .await?;
            write_err(stream, response_sequence, 1105, &message).await?;
        }
        Err(_) => {
            if enforce_row_limit {
                let _ = timeout(QUERY_TIMEOUT, upstream.query_drop("ROLLBACK")).await;
            }
            let message = "upstream MySQL query timed out".to_owned();
            write_audit(
                context,
                statement,
                Decision::Allowed,
                None,
                Some(message.clone()),
                rollback_artifact_id,
            )
            .await?;
            write_err(stream, response_sequence, 1205, &message).await?;
        }
    }
    Ok(())
}

fn is_row_limited_mutation(statement: &str) -> bool {
    let normalized = statement.trim_start().to_ascii_lowercase();
    normalized.starts_with("insert")
        || normalized.starts_with("update")
        || normalized.starts_with("delete")
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
