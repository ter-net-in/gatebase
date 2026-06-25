use crate::error::ApiError;
use crate::state::AppState;
use axum::http::HeaderMap;
use chrono::{Duration, Utc};
use gatebase_core::UserRole;
use jsonwebtoken::{decode, encode, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdminClaims {
    pub(crate) sub: String,
    pub(crate) username: String,
    pub(crate) role: UserRole,
    pub(crate) exp: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct AdminAuth {
    pub(crate) username: String,
    pub(crate) role: UserRole,
}

pub(crate) fn issue_admin_token(
    state: &AppState,
    id: String,
    username: String,
    role: UserRole,
) -> Result<String, ApiError> {
    let claims = AdminClaims {
        sub: id,
        username,
        role,
        exp: (Utc::now() + Duration::hours(8)).timestamp() as usize,
    };
    encode(&Header::default(), &claims, &state.admin_encoding_key).map_err(ApiError::internal)
}

pub(crate) async fn require_role(
    state: &AppState,
    headers: &HeaderMap,
    required: UserRole,
) -> Result<AdminAuth, ApiError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
    let claims = decode::<AdminClaims>(token, &state.admin_decoding_key, &Validation::default())
        .map_err(|_| ApiError::unauthorized("invalid bearer token"))?
        .claims;
    let user = state
        .store
        .find_user_by_username(&claims.username)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::unauthorized("invalid bearer token"))?;
    if user.disabled_at.is_some() || user.id != claims.sub {
        return Err(ApiError::unauthorized("invalid bearer token"));
    }
    if !user.role.can(required) {
        return Err(ApiError::from(axum::http::StatusCode::FORBIDDEN));
    }
    Ok(AdminAuth {
        username: user.username,
        role: user.role,
    })
}
