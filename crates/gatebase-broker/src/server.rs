use crate::routes::router;
use crate::state::AppState;
use anyhow::{Context, Result};
use gatebase_config::Config;
use gatebase_core::AccessSignal;
use gatebase_github::{GitHubAppConfig, GitHubProvider};
use gatebase_session::{SessionIssuer, SessionStore};
use std::fs;
use std::sync::Arc;
use tokio::net::TcpListener;

pub async fn run(config: Config) -> Result<()> {
    let signing_secret = fs::read(&config.sessions.signing_key_file).with_context(|| {
        format!(
            "failed to read signing key {}",
            config.sessions.signing_key_file.display()
        )
    })?;
    let store = SessionStore::open(&config.metadata.sqlite_path).await?;
    let requires_github = config.access.required_signals.iter().any(is_github_signal);
    let github = if requires_github {
        let github = config
            .github
            .as_ref()
            .context("GitHub access signals require github config")?;
        GitHubProvider::new(
            GitHubAppConfig::from_file(
                github.app_id.clone(),
                github.installation_id,
                &github.private_key_file,
                github.webhook_secret.clone(),
                github.api_base_url.clone(),
            )
            .await?,
        )
    } else {
        GitHubProvider::disabled()
    };

    let state = Arc::new(AppState {
        config: config.clone(),
        store,
        issuer: SessionIssuer::new(&signing_secret),
        github,
    });

    let listener = TcpListener::bind(config.server.broker_listen).await?;
    tracing::info!(listen = %config.server.broker_listen, "broker listening");
    axum::serve(listener, router(state)).await?;
    Ok(())
}

fn is_github_signal(signal: &AccessSignal) -> bool {
    matches!(
        signal,
        AccessSignal::GitHubPullRequestOpen
            | AccessSignal::GitHubPullRequestApproved
            | AccessSignal::GitHubChecksPassed { .. }
            | AccessSignal::GitHubLabels { .. }
            | AccessSignal::GitHubCodeownersReviewed
    )
}
