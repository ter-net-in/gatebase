use crate::entities;
use anyhow::Result;
use chrono::{DateTime, Utc};
use gatebase_core::{AccessApproval, ActiveConnection, Session, SessionId};

pub(crate) fn model_to_access_approval(
    model: entities::access_approval::Model,
) -> Result<AccessApproval> {
    Ok(AccessApproval {
        id: model.id,
        repo: model.repo,
        pull_request: model.pull_request,
        target: model.target,
        actor: model.actor,
        approver: model.approver,
        reason: model.reason,
        created_at: parse_time(&model.created_at)?,
        expires_at: model.expires_at.as_deref().map(parse_time).transpose()?,
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
        github_repo: model.github_repo,
        pull_request: model.pull_request,
        target: model.target,
        scopes: serde_json::from_str(&model.scopes)?,
        created_at: parse_time(&model.created_at)?,
        expires_at: parse_time(&model.expires_at)?,
        revoked_at: model.revoked_at.as_deref().map(parse_time).transpose()?,
    })
}

fn parse_time(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}
