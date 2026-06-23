use crate::cli::AuditCommand;
use anyhow::{Context, Result};
use gatebase_config::Config;
use gatebase_session::{AuditEventFilter, SessionStore};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditEventOutput {
    id: String,
    session_id: String,
    actor: String,
    target: String,
    engine: String,
    statement: String,
    decision: String,
    rows_affected: Option<i64>,
    error: Option<String>,
    created_at: String,
}

pub(crate) async fn run(command: AuditCommand) -> Result<()> {
    match command {
        AuditCommand::List {
            config,
            broker,
            actor,
            target,
            decision,
            limit,
            json,
        } => list(config, broker, actor, target, decision, limit, json).await,
    }
}

async fn list(
    config: Option<PathBuf>,
    broker: Option<String>,
    actor: Option<String>,
    target: Option<String>,
    decision: Option<String>,
    limit: u64,
    json: bool,
) -> Result<()> {
    anyhow::ensure!(
        config.is_some() ^ broker.is_some(),
        "provide exactly one of --config or --broker"
    );
    validate_decision(decision.as_deref())?;
    let events = if let Some(config) = config {
        list_from_config(config, actor, target, decision, limit).await?
    } else {
        list_from_broker(
            broker.expect("checked above"),
            actor,
            target,
            decision,
            limit,
        )
        .await?
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&events)?);
    } else {
        print_table(&events);
    }
    Ok(())
}

async fn list_from_config(
    config: PathBuf,
    actor: Option<String>,
    target: Option<String>,
    decision: Option<String>,
    limit: u64,
) -> Result<Vec<AuditEventOutput>> {
    let config = Config::load(config)?;
    let store = SessionStore::open(&config.metadata.sqlite_path).await?;
    Ok(store
        .list_audit_events(AuditEventFilter {
            actor,
            target,
            decision,
            limit: Some(limit),
        })
        .await?
        .into_iter()
        .map(|event| AuditEventOutput {
            id: event.id.to_string(),
            session_id: event.session_id.to_string(),
            actor: event.actor,
            target: event.target,
            engine: event.engine.to_string(),
            statement: event.statement,
            decision: format!("{:?}", event.decision).to_ascii_lowercase(),
            rows_affected: event.rows_affected,
            error: event.error,
            created_at: event.created_at.to_rfc3339(),
        })
        .collect())
}

async fn list_from_broker(
    broker: String,
    actor: Option<String>,
    target: Option<String>,
    decision: Option<String>,
    limit: u64,
) -> Result<Vec<AuditEventOutput>> {
    let mut url = reqwest::Url::parse(&format!(
        "{}/api/audit/events",
        broker.trim_end_matches('/')
    ))
    .context("invalid broker URL")?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("limit", &limit.to_string());
        if let Some(actor) = actor {
            pairs.append_pair("actor", &actor);
        }
        if let Some(target) = target {
            pairs.append_pair("target", &target);
        }
        if let Some(decision) = decision {
            pairs.append_pair("decision", &decision);
        }
    }
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to connect to broker {broker}"))?;
    let status = response.status();
    let body = response.text().await?;
    anyhow::ensure!(status.is_success(), "broker request failed: {body}");
    Ok(serde_json::from_str(&body)?)
}

fn validate_decision(decision: Option<&str>) -> Result<()> {
    if let Some(decision) = decision {
        anyhow::ensure!(
            matches!(decision, "allowed" | "blocked"),
            "--decision must be allowed or blocked"
        );
    }
    Ok(())
}

fn print_table(events: &[AuditEventOutput]) {
    println!("created_at\tactor\ttarget\tengine\tdecision\trows\tstatement");
    for event in events {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            event.created_at,
            event.actor,
            event.target,
            event.engine,
            event.decision,
            event
                .rows_affected
                .map(|rows| rows.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            event.statement.replace('\n', " ")
        );
    }
}
