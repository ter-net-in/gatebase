use crate::dto::{
    AuditEventResponse, AuditQuery, CreateSessionRequest, CreateSessionResponse,
    GitHubWebhookPayload, SessionResponse,
};
use crate::state::AppState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::Json;
use chrono::{Duration, Utc};
use gatebase_core::{AccessToken, SessionId};
use gatebase_github::{verify_webhook_signature, AccessRequest, GitProvider};
use gatebase_session::{hash_access_token, new_session, AuditEventFilter};
use std::sync::Arc;
use uuid::Uuid;

pub(crate) async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SessionResponse>>, String> {
    let sessions = state
        .store
        .list()
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
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEventResponse>>, String> {
    let events = state
        .store
        .list_audit_events(AuditEventFilter {
            actor: query.actor,
            target: query.target,
            decision: query.decision,
            limit: query.limit,
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
        })
        .collect();
    Ok(Json(events))
}

pub(crate) async fn revoke_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, String> {
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
    if let Err(error) = mint_issue_access_token(&state, &payload.repository.full_name, issue.number).await {
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
    let raw_token = format!("gb_at_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
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
