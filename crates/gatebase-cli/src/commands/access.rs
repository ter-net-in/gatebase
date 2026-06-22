use crate::cli::AccessCommand;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct CreateAccessApprovalRequest {
    repo: String,
    pull_request: Option<i64>,
    target: String,
    actor: Option<String>,
    approver: String,
    reason: Option<String>,
    ttl_minutes: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateAccessApprovalResponse {
    approval_id: String,
    expires_at: Option<String>,
}

pub(crate) async fn run(command: AccessCommand) -> Result<()> {
    match command {
        AccessCommand::Approve {
            broker,
            repo,
            pull_request,
            target,
            actor,
            approver,
            reason,
            ttl_minutes,
        } => {
            let response: CreateAccessApprovalResponse = post_json(
                &broker,
                "/api/access/approvals",
                &CreateAccessApprovalRequest {
                    repo,
                    pull_request,
                    target,
                    actor,
                    approver,
                    reason,
                    ttl_minutes,
                },
            )
            .await?;
            println!("approved {}", response.approval_id);
            if let Some(expires_at) = response.expires_at {
                println!("expires_at {expires_at}");
            }
            Ok(())
        }
    }
}

async fn post_json<T, R>(broker: &str, path: &str, body: &T) -> Result<R>
where
    T: Serialize,
    R: for<'de> Deserialize<'de>,
{
    let url = format!("{}{}", broker.trim_end_matches('/'), path);
    let response = reqwest::Client::new()
        .post(&url)
        .json(body)
        .send()
        .await
        .with_context(|| format!("failed to connect to broker {broker}"))?;
    let status = response.status();
    let body = response.text().await?;
    anyhow::ensure!(status.is_success(), "broker request failed: {body}");
    Ok(serde_json::from_str(&body)?)
}
