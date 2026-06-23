use crate::cli::SessionCommand;
use anyhow::{Context, Result};
use gatebase_config::Config;
use gatebase_core::SessionId;
use gatebase_session::{new_session, SessionIssuer, SessionStore};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Serialize)]
struct CreateSessionRequest {
    token: String,
}

#[derive(Debug, Deserialize)]
struct CreateSessionResponse {
    session_id: String,
    expires_at: String,
    connection_string: String,
}

pub(crate) async fn run(command: SessionCommand) -> Result<()> {
    match command {
        SessionCommand::Create { broker, token } => create(broker, token).await,
        SessionCommand::CreateLocal {
            config,
            target,
            actor,
        } => create_local(config, target, actor).await,
        SessionCommand::List { config } => list(config).await,
        SessionCommand::Revoke { config, id } => revoke(config, id).await,
    }
}

async fn create_local(
    config: std::path::PathBuf,
    target_name: String,
    actor: String,
) -> Result<()> {
    let config = Config::load(config)?;
    let target = config
        .targets
        .iter()
        .find(|target| target.name == target_name)
        .ok_or_else(|| anyhow::anyhow!("unknown target {target_name}"))?;
    anyhow::ensure!(
        target.access.allow_cli_sessions,
        "target {} does not allow local CLI sessions",
        target.name
    );
    let store = SessionStore::open(&config.metadata.sqlite_path).await?;
    let signing_secret = fs::read(&config.sessions.signing_key_file)?;
    let issuer = SessionIssuer::new(&signing_secret);
    let session = new_session(actor, "cli".to_owned(), None, None, target.name.clone(), 15);
    store.create(&session).await?;
    let token = issuer.issue(&session)?;
    let host = target
        .public_host
        .as_deref()
        .map(str::to_owned)
        .or_else(|| public_url_host(&config.server.public_url))
        .unwrap_or_else(|| target.listen.ip().to_string());
    let port = target.public_port.unwrap_or_else(|| target.listen.port());
    let scheme = match target.engine {
        gatebase_core::DbEngine::Postgres => "postgresql",
        gatebase_core::DbEngine::Mysql => "mysql",
    };
    println!("session_id {}", session.id);
    println!("expires_at {}", session.expires_at.to_rfc3339());
    println!(
        "connection_string {scheme}://{}:{}@{}:{}/{}",
        session.actor, token, host, port, target.database
    );
    Ok(())
}

fn public_url_host(public_url: &str) -> Option<String> {
    let without_scheme = public_url.split_once("://")?.1;
    let authority = without_scheme.split('/').next()?.split('@').next_back()?;
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split_once(']')?.0
    } else {
        authority.split(':').next()?
    };
    (!host.is_empty()).then(|| host.to_owned())
}

async fn create(broker: String, token: String) -> Result<()> {
    let response: CreateSessionResponse =
        post_json(&broker, "/api/sessions", &CreateSessionRequest { token }).await?;
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
            session.github_repo.as_deref().unwrap_or("-"),
            session
                .issue
                .map(|issue| issue.to_string())
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
