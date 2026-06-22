use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub actor: String,
    pub repo: String,
    pub pull_request: Option<i64>,
    pub target: String,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub expires_at: String,
    pub connection_string: String,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub session_id: String,
    pub actor: String,
    pub repo: String,
    pub pull_request: Option<i64>,
    pub target: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAccessApprovalRequest {
    pub repo: String,
    pub pull_request: Option<i64>,
    pub target: String,
    pub actor: Option<String>,
    pub approver: String,
    pub reason: Option<String>,
    pub ttl_minutes: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CreateAccessApprovalResponse {
    pub approval_id: String,
    pub expires_at: Option<String>,
}
