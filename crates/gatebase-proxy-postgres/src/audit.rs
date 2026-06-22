use anyhow::Result;
use chrono::Utc;
use gatebase_audit::{AuditSink, JsonlAuditSink, SqliteAuditSink};
use gatebase_config::{AuditSinkConfig, Config, TargetConfig};
use gatebase_core::{AuditEvent, AuditEventId, DbEngine, Decision, Session};
use gatebase_session::SessionStore;
use std::sync::Arc;

pub(crate) async fn build_sinks(
    config: &Config,
    store: &SessionStore,
) -> Result<Vec<Arc<dyn AuditSink>>> {
    let mut sinks: Vec<Arc<dyn AuditSink>> = Vec::new();
    for sink in &config.audit.sinks {
        match sink {
            AuditSinkConfig::Sqlite => sinks.push(Arc::new(
                SqliteAuditSink::new(store.metadata().clone()).await?,
            )),
            AuditSinkConfig::Jsonl { path } => {
                sinks.push(Arc::new(JsonlAuditSink::new(path.clone())))
            }
        }
    }
    Ok(sinks)
}

pub(crate) struct QueryContext<'a> {
    pub(crate) target: &'a TargetConfig,
    pub(crate) policy: &'a gatebase_config::PolicyConfig,
    pub(crate) sinks: &'a [Arc<dyn AuditSink>],
    pub(crate) session: &'a Session,
    pub(crate) fail_closed: bool,
}

pub(crate) async fn write_audit(
    context: &QueryContext<'_>,
    statement: &str,
    decision: Decision,
    rows_affected: Option<i64>,
    error: Option<String>,
) -> Result<()> {
    let event = AuditEvent {
        id: AuditEventId::new(),
        session_id: context.session.id.clone(),
        actor: context.session.actor.clone(),
        target: context.target.name.clone(),
        engine: DbEngine::Postgres,
        statement: statement.to_owned(),
        decision,
        rows_affected,
        error,
        created_at: Utc::now(),
    };
    for sink in context.sinks {
        if let Err(error) = sink.write(&event).await {
            if context.fail_closed {
                return Err(error);
            }
            tracing::warn!(%error, "audit sink write failed");
        }
    }
    Ok(())
}
