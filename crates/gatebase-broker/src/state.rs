use gatebase_config::Config;
use gatebase_github::GitHubProvider;
use gatebase_session::{SessionIssuer, SessionStore};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) config: Config,
    pub(crate) store: SessionStore,
    pub(crate) issuer: SessionIssuer,
    pub(crate) github: GitHubProvider,
}
