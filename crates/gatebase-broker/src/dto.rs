use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct AdminLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AdminLoginResponse {
    pub token: String,
    pub username: String,
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct AdminMeResponse {
    pub username: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub role: String,
    pub created_at: String,
    pub disabled_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PruneRequest {
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
pub struct PruneResponse {
    pub audit_events: u64,
    pub rollback_artifacts: u64,
    pub sessions: u64,
    pub access_tokens: u64,
    pub active_connections: u64,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub token: String,
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
    pub github_repo: String,
    pub issue: Option<i64>,
    pub target: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub actor: Option<String>,
    pub target: Option<String>,
    pub decision: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct AuditEventResponse {
    pub id: String,
    pub session_id: String,
    pub actor: String,
    pub target: String,
    pub engine: String,
    pub statement: String,
    pub decision: String,
    pub rows_affected: Option<i64>,
    pub error: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubWebhookPayload {
    pub action: Option<String>,
    pub issue: Option<GitHubWebhookIssue>,
    pub repository: GitHubWebhookRepository,
}

#[derive(Debug, Deserialize)]
pub struct GitHubWebhookIssue {
    pub number: i64,
}

#[derive(Debug, Deserialize)]
pub struct GitHubWebhookRepository {
    pub full_name: String,
}
