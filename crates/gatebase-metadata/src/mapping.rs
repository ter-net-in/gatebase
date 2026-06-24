use crate::entities;
use anyhow::Result;
use chrono::{DateTime, Utc};
use gatebase_core::{
    AccessToken, ActiveConnection, AuditEvent, AuditEventId, Decision, Session, SessionId, User,
    UserRole,
};
use std::str::FromStr;

pub(crate) fn model_to_access_token(model: entities::access_token::Model) -> Result<AccessToken> {
    Ok(AccessToken {
        id: model.id,
        token_hash: model.token_hash,
        actor: model.actor,
        github_repo: model.github_repo,
        issue: model.issue,
        target: model.target,
        created_at: parse_time(&model.created_at)?,
        expires_at: parse_time(&model.expires_at)?,
        used_at: model.used_at.as_deref().map(parse_time).transpose()?,
    })
}

pub(crate) fn model_to_active_connection(
    model: entities::active_connection::Model,
) -> Result<ActiveConnection> {
    Ok(ActiveConnection {
        id: model.id,
        session_id: SessionId::from(model.session_id),
        target: model.target,
        client_addr: model.client_addr,
        connected_at: parse_time(&model.connected_at)?,
        disconnected_at: model
            .disconnected_at
            .as_deref()
            .map(parse_time)
            .transpose()?,
    })
}

pub(crate) fn model_to_session(model: entities::session::Model) -> Result<Session> {
    Ok(Session {
        id: SessionId::from(model.id),
        actor: model.actor,
        source_type: model.source_type,
        github_repo: model.github_repo,
        issue: model.issue,
        target: model.target,
        scopes: serde_json::from_str(&model.scopes)?,
        created_at: parse_time(&model.created_at)?,
        expires_at: parse_time(&model.expires_at)?,
        revoked_at: model.revoked_at.as_deref().map(parse_time).transpose()?,
    })
}

pub(crate) fn model_to_audit_event(model: entities::audit_event::Model) -> Result<AuditEvent> {
    Ok(AuditEvent {
        id: AuditEventId::from(model.id),
        session_id: SessionId::from(model.session_id),
        actor: model.actor,
        target: model.target,
        engine: FromStr::from_str(&model.engine)?,
        statement: model.statement,
        decision: match model.decision.as_str() {
            "allowed" => Decision::Allowed,
            "blocked" => Decision::Blocked,
            other => anyhow::bail!("unknown audit decision {other}"),
        },
        rows_affected: model.rows_affected,
        error: model.error,
        created_at: parse_time(&model.created_at)?,
    })
}

pub(crate) fn model_to_user(model: entities::user::Model) -> Result<User> {
    Ok(User {
        id: model.id,
        username: model.username,
        password_hash: model.password_hash,
        role: UserRole::from_str(&model.role).map_err(anyhow::Error::msg)?,
        created_at: parse_time(&model.created_at)?,
        disabled_at: model.disabled_at.as_deref().map(parse_time).transpose()?,
    })
}

fn parse_time(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}
