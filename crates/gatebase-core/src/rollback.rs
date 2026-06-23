use crate::{DbEngine, SessionId};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RollbackArtifact {
    pub id: String,
    pub session_id: SessionId,
    pub actor: String,
    pub target: String,
    pub engine: DbEngine,
    pub statement: String,
    pub table: Option<String>,
    pub primary_key_column: Option<String>,
    pub before_rows: serde_json::Value,
    pub inverse_sql: Option<String>,
    pub manual_required: bool,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}
