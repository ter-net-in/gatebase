use crate::state::AppState;
use axum::http::{HeaderMap, StatusCode};
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
) -> Result<String, String> {
    let claims = AdminClaims {
        sub: id,
        username,
        role,
        exp: (Utc::now() + Duration::hours(8)).timestamp() as usize,
    };
    encode(&Header::default(), &claims, &state.admin_encoding_key)
        .map_err(|error| error.to_string())
}

pub(crate) fn require_role(
    state: &AppState,
    headers: &HeaderMap,
    required: UserRole,
) -> Result<AdminAuth, StatusCode> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let claims = decode::<AdminClaims>(token, &state.admin_decoding_key, &Validation::default())
        .map_err(|_| StatusCode::UNAUTHORIZED)?
        .claims;
    if !claims.role.can(required) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(AdminAuth {
        username: claims.username,
        role: claims.role,
    })
}
