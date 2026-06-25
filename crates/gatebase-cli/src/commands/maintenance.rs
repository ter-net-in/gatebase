use crate::cli::MaintenanceCommand;
use crate::settings;
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use gatebase_config::Config;
use gatebase_session::{PruneCutoffs, SessionStore};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct PruneRequest {
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
struct PruneResponse {
    audit_events: u64,
    rollback_artifacts: u64,
    sessions: u64,
    access_tokens: u64,
    active_connections: u64,
    total: u64,
}

pub(crate) async fn run(command: MaintenanceCommand) -> Result<()> {
    match command {
        MaintenanceCommand::Prune {
            config,
            broker,
            admin_token,
            dry_run,
        } => prune(config, broker, admin_token, dry_run).await,
    }
}

async fn prune(
    config: Option<std::path::PathBuf>,
    broker: Option<String>,
    admin_token: Option<String>,
    dry_run: bool,
) -> Result<()> {
    if let Some(config) = config {
        prune_local(config, dry_run).await
    } else {
        prune_broker(
            settings::broker(broker)?,
            settings::admin_token(admin_token)?,
            dry_run,
        )
        .await
    }
}

async fn prune_local(config: std::path::PathBuf, dry_run: bool) -> Result<()> {
    let config = Config::load(config)?;
    let now = Utc::now();
    let cutoffs = PruneCutoffs {
        audit_before: now - Duration::days(config.retention.audit_days as i64),
        rollback_before: now - Duration::days(config.retention.rollback_days as i64),
        session_before: now - Duration::days(config.retention.session_days as i64),
        approval_before: now - Duration::days(config.retention.approval_days as i64),
        active_connection_before: now
            - Duration::days(config.retention.active_connection_days as i64),
    };
    let store = SessionStore::open_metadata(&config.metadata).await?;
    let result = store.prune(&cutoffs, dry_run).await?;
    print_prune_result(
        if dry_run { "would_prune" } else { "pruned" },
        result.audit_events,
        result.rollback_artifacts,
        result.sessions,
        result.access_tokens,
        result.active_connections,
        result.total(),
    );
    Ok(())
}

async fn prune_broker(broker: String, admin_token: String, dry_run: bool) -> Result<()> {
    let response: PruneResponse = post_json_auth(
        &broker,
        "/api/admin/maintenance/prune",
        &admin_token,
        &PruneRequest { dry_run },
    )
    .await?;
    let prefix = if dry_run { "would_prune" } else { "pruned" };
    print_prune_result(
        prefix,
        response.audit_events,
        response.rollback_artifacts,
        response.sessions,
        response.access_tokens,
        response.active_connections,
        response.total,
    );
    Ok(())
}

fn print_prune_result(
    prefix: &str,
    audit_events: u64,
    rollback_artifacts: u64,
    sessions: u64,
    access_tokens: u64,
    active_connections: u64,
    total: u64,
) {
    println!("{prefix} audit_events {audit_events}");
    println!("{prefix} rollback_artifacts {rollback_artifacts}");
    println!("{prefix} sessions {sessions}");
    println!("{prefix} access_tokens {access_tokens}");
    println!("{prefix} active_connections {active_connections}");
    println!("{prefix} total {total}");
}

async fn post_json_auth<T, R>(broker: &str, path: &str, token: &str, body: &T) -> Result<R>
where
    T: Serialize,
    R: for<'de> Deserialize<'de>,
{
    let url = format!("{}{}", broker.trim_end_matches('/'), path);
    let response = reqwest::Client::new()
        .post(&url)
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .with_context(|| format!("failed to connect to broker {broker}"))?;
    let status = response.status();
    let body = response.text().await?;
    anyhow::ensure!(status.is_success(), "broker request failed: {body}");
    Ok(serde_json::from_str(&body)?)
}
