use crate::cli::MaintenanceCommand;
use anyhow::Result;
use chrono::{Duration, Utc};
use gatebase_config::Config;
use gatebase_session::{PruneCutoffs, SessionStore};

pub(crate) async fn run(command: MaintenanceCommand) -> Result<()> {
    match command {
        MaintenanceCommand::Prune { config, dry_run } => prune(config, dry_run).await,
    }
}

async fn prune(config: std::path::PathBuf, dry_run: bool) -> Result<()> {
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
    let store = SessionStore::open(&config.metadata.sqlite_path).await?;
    let result = store.prune(&cutoffs, dry_run).await?;
    let prefix = if dry_run { "would_prune" } else { "pruned" };
    println!("{prefix} audit_events {}", result.audit_events);
    println!("{prefix} rollback_artifacts {}", result.rollback_artifacts);
    println!("{prefix} sessions {}", result.sessions);
    println!("{prefix} access_tokens {}", result.access_tokens);
    println!("{prefix} active_connections {}", result.active_connections);
    println!("{prefix} total {}", result.total());
    Ok(())
}
