use chrono::{Duration, Utc};
use gatebase_core::{Session, SessionId};

pub fn new_session(
    actor: String,
    repo: String,
    pr: Option<i64>,
    target: String,
    ttl_minutes: i64,
) -> Session {
    let now = Utc::now();
    Session {
        id: SessionId::new(),
        actor,
        github_repo: repo,
        pull_request: pr,
        target,
        scopes: vec!["read".to_owned(), "write".to_owned()],
        created_at: now,
        expires_at: now + Duration::minutes(ttl_minutes),
        revoked_at: None,
    }
}
