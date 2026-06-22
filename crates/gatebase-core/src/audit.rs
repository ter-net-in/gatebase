use crate::{AuditEventId, DbEngine, Decision, SessionId};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditEvent {
    pub id: AuditEventId,
    pub session_id: SessionId,
    pub actor: String,
    pub target: String,
    pub engine: DbEngine,
    pub statement: String,
    pub decision: Decision,
    pub rows_affected: Option<i64>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}
