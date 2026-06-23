use crate::handlers::{
    create_session, github_webhook, healthz, list_audit_events, list_sessions, revoke_session,
};
use crate::state::AppState;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

pub(crate) fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(healthz))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:id/revoke", post(revoke_session))
        .route("/api/audit/events", get(list_audit_events))
        .route("/webhooks/github", post(github_webhook))
        .with_state(state)
}
