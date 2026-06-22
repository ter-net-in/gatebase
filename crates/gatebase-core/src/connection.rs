use crate::SessionId;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveConnection {
    pub id: String,
    pub session_id: SessionId,
    pub target: String,
    pub client_addr: String,
    pub connected_at: DateTime<Utc>,
    pub disconnected_at: Option<DateTime<Utc>>,
}
