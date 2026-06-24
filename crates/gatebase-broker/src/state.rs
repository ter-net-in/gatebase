use gatebase_config::Config;
use gatebase_github::GitHubProvider;
use gatebase_session::{SessionIssuer, SessionStore};
use jsonwebtoken::{DecodingKey, EncodingKey};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) config: Config,
    pub(crate) store: SessionStore,
    pub(crate) issuer: SessionIssuer,
    pub(crate) admin_encoding_key: EncodingKey,
    pub(crate) admin_decoding_key: DecodingKey,
    pub(crate) github: GitHubProvider,
}
