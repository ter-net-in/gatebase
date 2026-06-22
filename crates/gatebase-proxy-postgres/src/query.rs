use crate::audit::{write_audit, QueryContext};
use crate::protocol::{
    write_command_complete, write_data_row, write_empty_query, write_error, write_row_description,
};
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
                    SimpleQueryMessage::RowDescription(columns) => {
                        write_row_description(writer, &columns).await?;
                    }
                    _ => {}
                }
            }
            write_audit(context, statement, Decision::Allowed, rows_affected, None).await?;
        }
        Ok(Err(error)) => {
            let message = error.to_string();
            write_audit(
                context,
                statement,
                Decision::Allowed,
                None,
                Some(message.clone()),
            )
            .await?;
            write_error(writer, "XX000", &message).await?;
        }
        Err(_) => {
            let message = "upstream Postgres query timed out".to_owned();
            write_audit(
                context,
                statement,
                Decision::Allowed,
                None,
                Some(message.clone()),
            )
            .await?;
            write_error(writer, "57014", &message).await?;
        }
    }
    Ok(())
}
