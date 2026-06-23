use crate::claims::Claims;
use crate::SessionStore;
use anyhow::{Context, Result};
use chrono::Utc;
use gatebase_core::{Session, SessionId};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

#[derive(Clone)]
pub struct SessionIssuer {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

#[derive(Debug, Clone)]
pub struct VerifiedSession {
    pub token_session_id: SessionId,
    pub session: Session,
}

impl SessionIssuer {
    #[must_use]
    pub fn new(secret: &[u8]) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
        }
    }

    pub fn issue(&self, session: &Session) -> Result<String> {
        let claims = Claims {
            sub: session.actor.clone(),
            session_id: session.id.to_string(),
            source_type: session.source_type.clone(),
            github_repo: session.github_repo.clone(),
            issue: session.issue,
            target: session.target.clone(),
            scopes: session.scopes.clone(),
            exp: session.expires_at.timestamp() as usize,
        };
        encode(&Header::default(), &claims, &self.encoding_key)
            .context("failed to sign session token")
    }

    pub fn verify(&self, token: &str) -> Result<SessionId> {
        let claims = decode::<Claims>(token, &self.decoding_key, &Validation::default())?;
        Ok(SessionId::from(claims.claims.session_id))
    }

    pub async fn verify_active(
        &self,
        store: &SessionStore,
        token: &str,
        target: &str,
    ) -> Result<VerifiedSession> {
        let token_session_id = self.verify(token)?;
        let session = store
            .get(&token_session_id)
            .await?
            .with_context(|| format!("unknown session {token_session_id}"))?;
        anyhow::ensure!(
            session.target == target,
            "session is not valid for target {target}"
        );
        anyhow::ensure!(
            session.is_active(Utc::now()),
            "session is expired or revoked"
        );
        Ok(VerifiedSession {
            token_session_id,
            session,
        })
    }
}
