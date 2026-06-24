use crate::handlers::{
    admin_login, admin_me, create_session, create_user, get_audit_rollback, github_webhook,
    healthz, list_activity, list_audit_events, list_connections, list_rollbacks, list_sessions,
    list_users, prune, revoke_session,
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
        .route("/api/audit/events/:id/rollback", get(get_audit_rollback))
        .route("/api/rollbacks", get(list_rollbacks))
        .route("/api/connections", get(list_connections))
        .route("/api/activity", get(list_activity))
        .route("/api/admin/login", post(admin_login))
        .route("/api/admin/me", get(admin_me))
        .route("/api/admin/users", get(list_users).post(create_user))
        .route("/api/admin/maintenance/prune", post(prune))
        .route("/webhooks/github", post(github_webhook))
        .with_state(state)
}
