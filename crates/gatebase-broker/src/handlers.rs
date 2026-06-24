use crate::auth::{issue_admin_token, require_role};
use crate::dto::{
    ActiveConnectionResponse, ActivityResponse, AdminLoginRequest, AdminLoginResponse,
    AdminMeResponse, AuditEventResponse, AuditQuery, CreateSessionRequest, CreateSessionResponse,
    CreateUserRequest, GitHubWebhookPayload, Pagination, PruneRequest, PruneResponse,
    RollbackResponse, SessionResponse, UserResponse,
};
use crate::state::AppState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::Json;
use chrono::{Duration, Utc};
use gatebase_core::{AccessToken, SessionId, User, UserRole};
use gatebase_github::{verify_webhook_signature, AccessRequest, GitProvider};
use gatebase_session::{
    hash_access_token, new_session, verify_password, AuditEventFilter, PruneCutoffs,
};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

pub(crate) async fn list_sessions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(page): Query<Pagination>,
) -> Result<Json<Vec<SessionResponse>>, String> {
    require_role(&state, &headers, UserRole::Viewer).map_err(status_message)?;
    let sessions = state
        .store
        .list(page.limit, page.offset)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|session| SessionResponse {
            session_id: session.id.to_string(),
            actor: session.actor,
            github_repo: session.github_repo.unwrap_or_default(),
            issue: session.issue,
            target: session.target,
            expires_at: session.expires_at.to_rfc3339(),
            revoked_at: session.revoked_at.map(|time| time.to_rfc3339()),
        })
        .collect();
    Ok(Json(sessions))
}

pub(crate) async fn list_audit_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEventResponse>>, String> {
    require_role(&state, &headers, UserRole::Viewer).map_err(status_message)?;
    let events = state
        .store
        .list_audit_events(AuditEventFilter {
            actor: query.actor,
            target: query.target,
            decision: query.decision,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|event| AuditEventResponse {
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
            rollback_artifact_id: event.rollback_artifact_id,
        })
        .collect();
    Ok(Json(events))
}

pub(crate) async fn get_audit_rollback(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RollbackResponse>, StatusCode> {
    require_role(&state, &headers, UserRole::Viewer)?;
    let event = state
        .store
        .find_audit_event(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let rollback_id = event.rollback_artifact_id.ok_or(StatusCode::NOT_FOUND)?;
    let artifact = state
        .store
        .find_rollback_artifact(&rollback_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(RollbackResponse {
        id: artifact.id,
        session_id: artifact.session_id.to_string(),
        actor: artifact.actor,
        target: artifact.target,
        engine: artifact.engine.to_string(),
        statement: artifact.statement,
        table_name: artifact.table,
        primary_key_column: artifact.primary_key_column,
        inverse_sql: artifact.inverse_sql,
        manual_required: artifact.manual_required,
        reason: artifact.reason,
        created_at: artifact.created_at.to_rfc3339(),
    }))
}

pub(crate) async fn list_rollbacks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(page): Query<Pagination>,
) -> Result<Json<Vec<RollbackResponse>>, String> {
    require_role(&state, &headers, UserRole::Viewer).map_err(status_message)?;
    let artifacts = state
        .store
        .list_rollback_artifacts(page.limit, page.offset)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|artifact| RollbackResponse {
            id: artifact.id,
            session_id: artifact.session_id.to_string(),
            actor: artifact.actor,
            target: artifact.target,
            engine: artifact.engine.to_string(),
            statement: artifact.statement,
            table_name: artifact.table,
            primary_key_column: artifact.primary_key_column,
            inverse_sql: artifact.inverse_sql,
            manual_required: artifact.manual_required,
            reason: artifact.reason,
            created_at: artifact.created_at.to_rfc3339(),
        })
        .collect();
    Ok(Json(artifacts))
}

pub(crate) async fn list_connections(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(page): Query<Pagination>,
) -> Result<Json<Vec<ActiveConnectionResponse>>, String> {
    require_role(&state, &headers, UserRole::Viewer).map_err(status_message)?;
    let connections = state
        .store
        .list_active_connections(page.limit, page.offset)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|connection| ActiveConnectionResponse {
            id: connection.id,
            session_id: connection.session_id.to_string(),
            target: connection.target,
            client_addr: connection.client_addr,
            connected_at: connection.connected_at.to_rfc3339(),
            disconnected_at: connection.disconnected_at.map(|time| time.to_rfc3339()),
        })
        .collect();
    Ok(Json(connections))
}

pub(crate) async fn list_activity(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(page): Query<Pagination>,
) -> Result<Json<Vec<ActivityResponse>>, String> {
    require_role(&state, &headers, UserRole::Viewer).map_err(status_message)?;
    let entries = state
        .store
        .list_activity(page.limit, page.offset)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|entry| ActivityResponse {
            time: entry.time,
            category: entry.category,
            actor: entry.actor,
            target: entry.target,
            detail: entry.detail,
        })
        .collect();
    Ok(Json(entries))
}

pub(crate) async fn revoke_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, String> {
    require_role(&state, &headers, UserRole::Operator).map_err(status_message)?;
    state
        .store
        .revoke(&SessionId::from(id))
        .await
        .map_err(|error| error.to_string())?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn healthz() -> &'static str {
    "ok"
}

pub(crate) async fn admin_login(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AdminLoginRequest>,
) -> Result<Json<AdminLoginResponse>, String> {
    let user = state
        .store
        .find_user_by_username(&request.username)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "invalid username or password".to_owned())?;
    if user.disabled_at.is_some()
        || !verify_password(&request.password, &user.password_hash)
            .map_err(|error| error.to_string())?
    {
        return Err("invalid username or password".to_owned());
    }
    let token = issue_admin_token(&state, user.id, user.username.clone(), user.role)?;
    Ok(Json(AdminLoginResponse {
        token,
        username: user.username,
        role: user.role.to_string(),
    }))
}

pub(crate) async fn admin_me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<AdminMeResponse>, StatusCode> {
    let auth = require_role(&state, &headers, UserRole::Viewer)?;
    Ok(Json(AdminMeResponse {
        username: auth.username,
        role: auth.role.to_string(),
    }))
}

pub(crate) async fn list_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(page): Query<Pagination>,
) -> Result<Json<Vec<UserResponse>>, String> {
    require_role(&state, &headers, UserRole::Admin).map_err(status_message)?;
    let users = state
        .store
        .list_users(page.limit, page.offset)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(user_response)
        .collect();
    Ok(Json(users))
}

pub(crate) async fn create_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>, String> {
    require_role(&state, &headers, UserRole::Admin).map_err(status_message)?;
    let role = UserRole::from_str(&request.role)?;
    let user = state
        .store
        .create_user(request.username, &request.password, role)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Json(user_response(user)))
}

pub(crate) async fn prune(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<PruneRequest>,
) -> Result<Json<PruneResponse>, String> {
    require_role(&state, &headers, UserRole::Admin).map_err(status_message)?;
    let now = Utc::now();
    let cutoffs = PruneCutoffs {
        audit_before: now - Duration::days(state.config.retention.audit_days as i64),
        rollback_before: now - Duration::days(state.config.retention.rollback_days as i64),
        session_before: now - Duration::days(state.config.retention.session_days as i64),
        approval_before: now - Duration::days(state.config.retention.approval_days as i64),
        active_connection_before: now
            - Duration::days(state.config.retention.active_connection_days as i64),
    };
    let result = state
        .store
        .prune(&cutoffs, request.dry_run)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Json(PruneResponse {
        audit_events: result.audit_events,
        rollback_artifacts: result.rollback_artifacts,
        sessions: result.sessions,
        access_tokens: result.access_tokens,
        active_connections: result.active_connections,
        total: result.total(),
    }))
}

pub(crate) async fn github_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let github = state.config.github.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !verify_webhook_signature(&github.webhook_secret, &body, signature) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let event = headers
        .get("x-github-event")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    if event != "issues" {
        return Ok(StatusCode::ACCEPTED);
    }
    let payload: GitHubWebhookPayload =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    let Some(action) = payload.action.as_deref() else {
        return Ok(StatusCode::ACCEPTED);
    };
    if !matches!(action, "opened" | "reopened" | "labeled" | "edited") {
        return Ok(StatusCode::ACCEPTED);
    }
    let Some(issue) = payload.issue else {
        return Ok(StatusCode::ACCEPTED);
    };
    if let Err(error) =
        mint_issue_access_token(&state, &payload.repository.full_name, issue.number).await
    {
        tracing::warn!(%error, "failed to mint issue access token");
    }
    Ok(StatusCode::ACCEPTED)
}

pub(crate) async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, String> {
    let access_token = state
        .store
        .consume_access_token(&request.token)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "invalid or expired access token".to_owned())?;

    let session = new_session(
        access_token.actor.clone(),
        "github_issue".to_owned(),
        Some(access_token.github_repo.clone()),
        Some(access_token.issue),
        access_token.target.clone(),
        15,
    );
    state
        .store
        .create(&session)
        .await
        .map_err(|error| error.to_string())?;
    let token = state
        .issuer
        .issue(&session)
        .map_err(|error| error.to_string())?;
    let target = state
        .config
        .targets
        .iter()
        .find(|target| target.name == access_token.target)
        .ok_or_else(|| format!("unknown target {}", access_token.target))?;
    let host = target
        .public_host
        .as_deref()
        .map(str::to_owned)
        .or_else(|| public_url_host(&state.config.server.public_url))
        .unwrap_or_else(|| target.listen.ip().to_string());
    let port = target.public_port.unwrap_or_else(|| target.listen.port());
    let connection_string = format!(
        "postgresql://{}:{}@{}:{}/{}",
        session.actor, token, host, port, target.database
    );

    Ok(Json(CreateSessionResponse {
        session_id: session.id.to_string(),
        expires_at: session.expires_at.to_rfc3339(),
        connection_string,
    }))
}

async fn mint_issue_access_token(state: &AppState, repo: &str, issue: i64) -> Result<(), String> {
    let target = state
        .config
        .targets
        .iter()
        .find(|target| target.access.github_repo == repo)
        .ok_or_else(|| format!("no target configured for repo {repo}"))?;
    let access = AccessRequest {
        github_repo: repo.to_owned(),
        issue,
        target: target.name.clone(),
    };
    ensure_access_allowed(state, target, &access).await?;
    let now = Utc::now();
    if state
        .store
        .find_active_access_token(repo, issue, &target.name)
        .await
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Ok(());
    }
    let issue_data = state
        .github
        .issue(repo, issue)
        .await
        .map_err(|error| error.to_string())?;
    let raw_token = format!(
        "gb_at_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let expires_at = now + Duration::minutes(parse_minutes(&target.access.access_token_ttl)?);
    let token = AccessToken {
        id: format!("at_{}", Uuid::new_v4().simple()),
        token_hash: hash_access_token(&raw_token),
        actor: issue_data.user.login,
        github_repo: repo.to_owned(),
        issue,
        target: target.name.clone(),
        created_at: now,
        expires_at,
        used_at: None,
    };
    state
        .store
        .create_access_token(&token)
        .await
        .map_err(|error| error.to_string())?;
    let body = format!(
        "Gatebase access approved.\n\nToken:\n{raw_token}\n\nUse:\n`gatebase session create --token {raw_token}`\n\nExpires at {}.",
        expires_at.to_rfc3339()
    );
    state
        .github
        .comment_issue(repo, issue, &body)
        .await
        .map_err(|error| error.to_string())?;
    state
        .github
        .close_issue(repo, issue)
        .await
        .map_err(|error| error.to_string())?;
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

async fn ensure_access_allowed(
    state: &AppState,
    target: &gatebase_config::TargetConfig,
    access: &AccessRequest,
) -> Result<(), String> {
    for signal in &target.access.required_signals {
        let evaluation = state
            .github
            .evaluate_signal(access, signal)
            .await
            .map_err(|error| error.to_string())?;
        if !evaluation.allowed {
            return Err(evaluation
                .reason
                .unwrap_or_else(|| format!("required access signal {signal:?} denied")));
        }
    }

    Ok(())
}

fn parse_minutes(value: &str) -> Result<i64, String> {
    value
        .strip_suffix('m')
        .unwrap_or(value)
        .parse::<i64>()
        .map_err(|_| format!("invalid minute duration {value}"))
}

fn user_response(user: User) -> UserResponse {
    UserResponse {
        id: user.id,
        username: user.username,
        role: user.role.to_string(),
        created_at: user.created_at.to_rfc3339(),
        disabled_at: user.disabled_at.map(|time| time.to_rfc3339()),
    }
}

fn status_message(status: StatusCode) -> String {
    status.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_public_url_host_for_connection_string_fallback() {
        assert_eq!(
            public_url_host("https://gatebase.example.com"),
            Some("gatebase.example.com".to_owned())
        );
        assert_eq!(
            public_url_host("http://127.0.0.1:8080"),
            Some("127.0.0.1".to_owned())
        );
        assert_eq!(
            public_url_host("https://user:pass@gatebase.example.com/path"),
            Some("gatebase.example.com".to_owned())
        );
    }
}
