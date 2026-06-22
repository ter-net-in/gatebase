use crate::cli::SessionCommand;
use anyhow::{Context, Result};
use gatebase_config::Config;
use gatebase_core::SessionId;
use gatebase_session::SessionStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct CreateSessionRequest {
    actor: String,
    repo: String,
    pull_request: Option<i64>,
    target: String,
}

#[derive(Debug, Deserialize)]
struct CreateSessionResponse {
    session_id: String,
    expires_at: String,
    connection_string: String,
}

pub(crate) async fn run(command: SessionCommand) -> Result<()> {
    match command {
        SessionCommand::Create {
            broker,
            actor,
            repo,
            pull_request,
            target,
        } => create(broker, actor, repo, pull_request, target).await,
        SessionCommand::List { config } => list(config).await,
        SessionCommand::Revoke { config, id } => revoke(config, id).await,
    }
}

async fn create(
    broker: String,
    actor: String,
    repo: String,
    pull_request: Option<i64>,
    target: String,
) -> Result<()> {
    let response: CreateSessionResponse = post_json(
        &broker,
        "/api/sessions",
        &CreateSessionRequest {
            actor,
            repo,
            pull_request,
            target,
        },
    )
    .await?;
    println!("session_id {}", response.session_id);
    println!("expires_at {}", response.expires_at);
    println!("connection_string {}", response.connection_string);
    Ok(())
}

async fn list(config: std::path::PathBuf) -> Result<()> {
    let config = Config::load(config)?;
    let store = SessionStore::open(&config.metadata.sqlite_path).await?;
    for session in store.list().await? {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            session.id,
            session.actor,
            session.github_repo,
            session
                .pull_request
                .map(|pull_request| pull_request.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            session.target,
            if session.is_active(chrono::Utc::now()) {
                "active"
            } else {
                "inactive"
            }
        );
    }
    Ok(())
}

async fn revoke(config: std::path::PathBuf, id: String) -> Result<()> {
    let config = Config::load(config)?;
    let store = SessionStore::open(&config.metadata.sqlite_path).await?;
    store.revoke(&SessionId::from(id.clone())).await?;
    println!("revoked {id}");
    Ok(())
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
