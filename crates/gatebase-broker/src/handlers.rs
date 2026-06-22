use crate::dto::{
    CreateAccessApprovalRequest, CreateAccessApprovalResponse, CreateSessionRequest,
    CreateSessionResponse, SessionResponse,
};
use crate::state::AppState;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::Json;
use chrono::{Duration, Utc};
use gatebase_core::{AccessApproval, AccessSignal, SessionId};
use gatebase_github::{verify_webhook_signature, AccessRequest, GitProvider};
use gatebase_session::new_session;
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
            repo: session.github_repo,
            pull_request: session.pull_request,
            target: session.target,
            expires_at: session.expires_at.to_rfc3339(),
            revoked_at: session.revoked_at.map(|time| time.to_rfc3339()),
        })
        .collect();
    Ok(Json(sessions))
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
    Ok(StatusCode::ACCEPTED)
}

pub(crate) async fn create_access_approval(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateAccessApprovalRequest>,
) -> Result<Json<CreateAccessApprovalResponse>, String> {
    let now = Utc::now();
    let approval = AccessApproval {
        id: format!("appr_{}", Uuid::new_v4().simple()),
        repo: request.repo,
        pull_request: request.pull_request,
        target: request.target,
        actor: request.actor,
        approver: request.approver,
        reason: request.reason,
        created_at: now,
        expires_at: request
            .ttl_minutes
            .map(|minutes| now + Duration::minutes(minutes)),
    };
    state
        .store
        .create_access_approval(&approval)
        .await
        .map_err(|error| error.to_string())?;

    Ok(Json(CreateAccessApprovalResponse {
        approval_id: approval.id,
        expires_at: approval.expires_at.map(|time| time.to_rfc3339()),
    }))
}

pub(crate) async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, String> {
    let access = AccessRequest {
        actor: request.actor.clone(),
        repo: request.repo.clone(),
        pull_request: request.pull_request,
        target: request.target.clone(),
    };
    ensure_access_allowed(&state, &access).await?;

    let session = new_session(
        request.actor,
        request.repo,
        request.pull_request,
        request.target.clone(),
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
        .find(|target| target.name == request.target)
        .ok_or_else(|| format!("unknown target {}", request.target))?;
    let host = target
        .public_host
        .as_deref()
        .map(str::to_owned)
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

async fn ensure_access_allowed(state: &AppState, access: &AccessRequest) -> Result<(), String> {
    if !state.config.access.allowed_repositories.is_empty()
        && !state
            .config
            .access
            .allowed_repositories
            .iter()
            .any(|repo| repo == &access.repo)
    {
        return Err(format!("repository {} is not allowed", access.repo));
    }

    for signal in &state.config.access.required_signals {
        if let AccessSignal::CliApproval {
            approvers,
            allow_without_pull_request,
        } = signal
        {
            if access.pull_request.is_none() && !allow_without_pull_request {
                return Err(
                    "required CLI approval is not configured to allow requests without pull requests"
                        .to_owned(),
                );
            }
            let approval = state
                .store
                .find_active_access_approval(
                    &access.repo,
                    access.pull_request,
                    &access.target,
                    &access.actor,
                    approvers,
                )
                .await
                .map_err(|error| error.to_string())?;
            if approval.is_none() {
                return Err("required CLI approval not found".to_owned());
            }
            continue;
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use gatebase_config::{
        AccessConfig, AuditConfig, Config, MetadataConfig, ServerConfig, SessionsConfig,
    };
    use gatebase_github::GitHubProvider;
    use gatebase_session::{SessionIssuer, SessionStore};
    use std::collections::HashMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    const REPO: &str = "gatebase/gatebase";
    const TARGET: &str = "prod-pg";
    const APPROVER: &str = "security-oncall";

    async fn test_store() -> SessionStore {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "gatebase-broker-test-{}-{nanos}.db",
            std::process::id()
        ));
        // `open` creates the file and runs migrations.
        SessionStore::open(path).await.unwrap()
    }

    fn state_with_signal(signal: AccessSignal, store: SessionStore) -> AppState {
        let config = Config {
            server: ServerConfig {
                public_url: "http://localhost:8080".to_owned(),
                broker_listen: "127.0.0.1:8080".parse().unwrap(),
            },
            metadata: MetadataConfig {
                sqlite_path: "unused.db".into(),
            },
            sessions: SessionsConfig {
                default_ttl: "15m".to_owned(),
                max_ttl: "30m".to_owned(),
                signing_key_file: "unused.key".into(),
            },
            github: None,
            access: AccessConfig {
                allowed_repositories: Vec::new(),
                required_signals: vec![signal],
            },
            audit: AuditConfig {
                fail_closed: true,
                sinks: Vec::new(),
            },
            targets: Vec::new(),
            policies: HashMap::new(),
        };
        AppState {
            config,
            store,
            issuer: SessionIssuer::new(b"test-secret"),
            github: GitHubProvider::disabled(),
        }
    }

    fn cli_approval(allow_without_pull_request: bool) -> AccessSignal {
        AccessSignal::CliApproval {
            approvers: vec![APPROVER.to_owned()],
            allow_without_pull_request,
        }
    }

    fn request_without_pr() -> AccessRequest {
        AccessRequest {
            actor: "alice".to_owned(),
            repo: REPO.to_owned(),
            pull_request: None,
            target: TARGET.to_owned(),
        }
    }

    async fn insert_no_pr_approval(store: &SessionStore) {
        store
            .create_access_approval(&AccessApproval {
                id: Uuid::new_v4().to_string(),
                repo: REPO.to_owned(),
                pull_request: None,
                target: TARGET.to_owned(),
                actor: None,
                approver: APPROVER.to_owned(),
                reason: None,
                created_at: Utc::now(),
                expires_at: None,
            })
            .await
            .unwrap();
    }

    // Flag false (the default): a request without a PR is rejected by the gate
    // before any approval lookup, even when a matching approval exists.
    #[tokio::test]
    async fn denies_request_without_pr_when_flag_is_false() {
        let store = test_store().await;
        insert_no_pr_approval(&store).await;
        let state = state_with_signal(cli_approval(false), store);

        let error = ensure_access_allowed(&state, &request_without_pr())
            .await
            .expect_err("no-PR request must be denied when the flag is false");
        assert!(
            error.contains("without pull requests"),
            "unexpected error: {error}"
        );
    }

    // Flag true: a request without a PR is allowed when a matching approval exists.
    #[tokio::test]
    async fn allows_request_without_pr_when_flag_true_and_approval_exists() {
        let store = test_store().await;
        insert_no_pr_approval(&store).await;
        let state = state_with_signal(cli_approval(true), store);

        let result = ensure_access_allowed(&state, &request_without_pr()).await;
        assert!(result.is_ok(), "expected access allowed, got {result:?}");
    }

    // Flag true but no approval recorded: the request is still denied.
    #[tokio::test]
    async fn denies_request_without_pr_when_flag_true_but_no_approval() {
        let store = test_store().await;
        let state = state_with_signal(cli_approval(true), store);

        let error = ensure_access_allowed(&state, &request_without_pr())
            .await
            .expect_err("no-PR request must be denied without a matching approval");
        assert!(
            error.contains("approval not found"),
            "unexpected error: {error}"
        );
    }
}
