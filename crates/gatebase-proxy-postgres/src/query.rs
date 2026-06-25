use crate::audit::{write_audit, QueryContext};
use crate::protocol::{
    write_command_complete, write_data_row, write_empty_query, write_error, write_no_data,
    write_row_description,
};
use crate::rollback::{capture_rollback_artifact, RollbackContext};
use anyhow::Result;
use gatebase_core::Decision;
use gatebase_policy::decide;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::time::{timeout, Duration};
use tokio_postgres::SimpleQueryMessage;

const IO_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn handle_query(
    statement: &str,
    upstream: &tokio_postgres::Client,
    writer: &mut OwnedWriteHalf,
    context: &QueryContext<'_>,
    rollback: &RollbackContext<'_>,
) -> Result<()> {
    handle_query_with_row_description(statement, upstream, writer, context, rollback, true).await
}

pub(crate) async fn handle_extended_query(
    statement: &str,
    upstream: &tokio_postgres::Client,
    writer: &mut OwnedWriteHalf,
    context: &QueryContext<'_>,
    rollback: &RollbackContext<'_>,
    emit_row_description: bool,
) -> Result<()> {
    handle_query_with_row_description(
        statement,
        upstream,
        writer,
        context,
        rollback,
        emit_row_description,
    )
    .await
}

pub(crate) async fn describe_query(
    statement: &str,
    upstream: &tokio_postgres::Client,
    writer: &mut OwnedWriteHalf,
    context: &QueryContext<'_>,
) -> Result<bool> {
    if statement.trim().is_empty()
        || !statement
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("select")
    {
        write_no_data(writer).await?;
        return Ok(false);
    }

    let policy_decision = decide(statement, context.policy);
    if policy_decision.decision == Decision::Blocked {
        write_error(
            writer,
            "42501",
            policy_decision
                .reason
                .as_deref()
                .unwrap_or("statement blocked by Gatebase policy"),
        )
        .await?;
        return Ok(false);
    }

    let describe_statement = format!("SELECT * FROM ({statement}) AS gatebase_describe LIMIT 0");
    match timeout(IO_TIMEOUT, upstream.simple_query(&describe_statement)).await {
        Ok(Ok(messages)) => {
            for message in messages {
                if let SimpleQueryMessage::RowDescription(columns) = message {
                    write_row_description(writer, &columns).await?;
                    return Ok(true);
                }
            }
            write_no_data(writer).await?;
            Ok(false)
        }
        Ok(Err(error)) => {
            write_error(writer, "XX000", &error.to_string()).await?;
            Ok(false)
        }
        Err(_) => {
            write_error(writer, "57014", "upstream Postgres describe timed out").await?;
            Ok(false)
        }
    }
}

async fn handle_query_with_row_description(
    statement: &str,
    upstream: &tokio_postgres::Client,
    writer: &mut OwnedWriteHalf,
    context: &QueryContext<'_>,
    rollback: &RollbackContext<'_>,
    emit_row_description: bool,
) -> Result<()> {
    if statement.trim().is_empty() {
        write_empty_query(writer).await?;
        return Ok(());
    }

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
        write_error(
            writer,
            "42501",
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
        timeout(IO_TIMEOUT, upstream.simple_query("BEGIN")).await??;
    }

    match timeout(IO_TIMEOUT, upstream.simple_query(statement)).await {
        Ok(Ok(messages)) => {
            let mut rows_affected = None;
            for message in messages {
                match message {
                    SimpleQueryMessage::Row(row) => write_data_row(writer, &row).await?,
                    SimpleQueryMessage::CommandComplete(count) => {
                        rows_affected = Some(count as i64);
                        write_command_complete(writer, statement, count).await?;
                    }
                    SimpleQueryMessage::RowDescription(columns) if emit_row_description => {
                        write_row_description(writer, &columns).await?;
                    }
                    _ => {}
                }
            }
            if let Some(limit) = context.policy.max_rows_changed {
                if enforce_row_limit && rows_affected.is_some_and(|rows| rows as u64 > limit) {
                    timeout(IO_TIMEOUT, upstream.simple_query("ROLLBACK")).await??;
                    let message = format!("statement changed more than max_rows_changed {limit}");
                    write_audit(
                        context,
                        statement,
                        Decision::Blocked,
                        rows_affected,
                        Some(message.clone()),
                        rollback_artifact_id,
                    )
                    .await?;
                    write_error(writer, "42501", &message).await?;
                    return Ok(());
                }
            }
            if enforce_row_limit {
                timeout(IO_TIMEOUT, upstream.simple_query("COMMIT")).await??;
            }
            write_audit(
                context,
                statement,
                Decision::Allowed,
                rows_affected,
                None,
                rollback_artifact_id,
            )
            .await?;
        }
        Ok(Err(error)) => {
            if enforce_row_limit {
                let _ = timeout(IO_TIMEOUT, upstream.simple_query("ROLLBACK")).await;
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
            write_error(writer, "XX000", &message).await?;
        }
        Err(_) => {
            if enforce_row_limit {
                let _ = timeout(IO_TIMEOUT, upstream.simple_query("ROLLBACK")).await;
            }
            let message = "upstream Postgres query timed out".to_owned();
            write_audit(
                context,
                statement,
                Decision::Allowed,
                None,
                Some(message.clone()),
                rollback_artifact_id,
            )
            .await?;
            write_error(writer, "57014", &message).await?;
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
