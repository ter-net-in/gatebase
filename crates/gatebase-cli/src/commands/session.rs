use crate::cli::SessionCommand;
use crate::settings;
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

#[derive(Debug, Deserialize)]
struct SessionResponse {
    session_id: String,
    actor: String,
    github_repo: String,
    issue: Option<i64>,
    target: String,
    expires_at: String,
    revoked_at: Option<String>,
}

pub(crate) async fn run(command: SessionCommand) -> Result<()> {
    match command {
        SessionCommand::Create { broker, token } => create(broker, token).await,
        SessionCommand::CreateLocal {
            config,
            target,
            actor,
        } => create_local(config, target, actor).await,
        SessionCommand::List {
            config,
            broker,
            admin_token,
        } => list(config, broker, admin_token).await,
        SessionCommand::Revoke {
            config,
            broker,
            admin_token,
            id,
        } => revoke(config, broker, admin_token, id).await,
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

async fn create(broker: Option<String>, token: String) -> Result<()> {
    let broker = settings::broker_or_localhost(broker)?;
    let response: CreateSessionResponse =
        post_json(&broker, "/api/sessions", &CreateSessionRequest { token }).await?;
    println!("session_id {}", response.session_id);
    println!("expires_at {}", response.expires_at);
    println!("connection_string {}", response.connection_string);
    Ok(())
}

async fn list(
    config: Option<std::path::PathBuf>,
    broker: Option<String>,
    admin_token: Option<String>,
) -> Result<()> {
    if let Some(config) = config {
        list_local(config).await
    } else {
        list_broker(
            settings::broker(broker)?,
            settings::admin_token(admin_token)?,
        )
        .await
    }
}

async fn list_local(config: std::path::PathBuf) -> Result<()> {
    let config = Config::load(config)?;
    let store = SessionStore::open(&config.metadata.sqlite_path).await?;
    for session in store.list(None, None).await? {
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

async fn list_broker(broker: String, admin_token: String) -> Result<()> {
    for session in
        get_json_auth::<Vec<SessionResponse>>(&broker, "/api/sessions", &admin_token).await?
    {
        let status = if session.revoked_at.is_none()
            && chrono::DateTime::parse_from_rfc3339(&session.expires_at)
                .map(|expires_at| expires_at.with_timezone(&chrono::Utc) > chrono::Utc::now())
                .unwrap_or(false)
        {
            "active"
        } else {
            "inactive"
        };
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            session.session_id,
            session.actor,
            if session.github_repo.is_empty() {
                "-"
            } else {
                &session.github_repo
            },
            session
                .issue
                .map(|issue| issue.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            session.target,
            status
        );
    }
    Ok(())
}

async fn revoke(
    config: Option<std::path::PathBuf>,
    broker: Option<String>,
    admin_token: Option<String>,
    id: String,
) -> Result<()> {
    if let Some(config) = config {
        revoke_local(config, id).await
    } else {
        revoke_broker(
            settings::broker(broker)?,
            settings::admin_token(admin_token)?,
            id,
        )
        .await
    }
}

async fn revoke_local(config: std::path::PathBuf, id: String) -> Result<()> {
    let config = Config::load(config)?;
    let store = SessionStore::open(&config.metadata.sqlite_path).await?;
    store.revoke(&SessionId::from(id.clone())).await?;
    println!("revoked {id}");
    Ok(())
}

async fn revoke_broker(broker: String, admin_token: String, id: String) -> Result<()> {
    post_empty_auth(&broker, &format!("/api/sessions/{id}/revoke"), &admin_token).await?;
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

async fn get_json_auth<R>(broker: &str, path: &str, token: &str) -> Result<R>
where
    R: for<'de> Deserialize<'de>,
{
    let url = format!("{}{}", broker.trim_end_matches('/'), path);
    let response = reqwest::Client::new()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("failed to connect to broker {broker}"))?;
    let status = response.status();
    let body = response.text().await?;
    anyhow::ensure!(status.is_success(), "broker request failed: {body}");
    Ok(serde_json::from_str(&body)?)
}

async fn post_empty_auth(broker: &str, path: &str, token: &str) -> Result<()> {
    let url = format!("{}{}", broker.trim_end_matches('/'), path);
    let response = reqwest::Client::new()
        .post(&url)
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("failed to connect to broker {broker}"))?;
    let status = response.status();
    let body = response.text().await?;
    anyhow::ensure!(status.is_success(), "broker request failed: {body}");
    Ok(())
}
