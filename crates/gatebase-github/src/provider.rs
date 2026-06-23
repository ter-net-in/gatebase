use crate::{AccessRequest, SignalEvaluation};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use gatebase_core::AccessSignal;
use hmac::{Hmac, Mac};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{fs, path::Path};

#[async_trait]
pub trait GitProvider: Send + Sync {
    async fn issue(&self, repo: &str, issue: i64) -> Result<Issue>;
    async fn evaluate_signal(
        &self,
        request: &AccessRequest,
        signal: &AccessSignal,
    ) -> Result<SignalEvaluation>;
    async fn comment_issue(&self, repo: &str, issue: i64, body: &str) -> Result<()>;
    async fn close_issue(&self, repo: &str, issue: i64) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct GitHubAppConfig {
    pub app_id: String,
    pub installation_id: i64,
    pub private_key_pem: Vec<u8>,
    pub webhook_secret: String,
    pub api_base_url: String,
}

impl GitHubAppConfig {
    pub async fn from_file(
        app_id: String,
        installation_id: i64,
        private_key_file: impl AsRef<Path>,
        webhook_secret: String,
        api_base_url: String,
    ) -> Result<Self> {
        let private_key_file = private_key_file.as_ref();
        let private_key_pem = fs::read(private_key_file).with_context(|| {
            format!(
                "failed to read GitHub App private key {}",
                private_key_file.display()
            )
        })?;
        Ok(Self {
            app_id,
            installation_id,
            private_key_pem,
            webhook_secret,
            api_base_url,
        })
    }
}

#[derive(Debug, Clone)]
pub struct GitHubProvider {
    client: Client,
    config: Option<GitHubAppConfig>,
}

impl GitHubProvider {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            client: Client::new(),
            config: None,
        }
    }

    #[must_use]
    pub fn new(config: GitHubAppConfig) -> Self {
        Self {
            client: Client::new(),
            config: Some(config),
        }
    }

    async fn get<T>(&self, path: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let config = self
            .config
            .as_ref()
            .context("GitHub App is not configured")?;
        let token = self.installation_token().await?;
        let url = format!("{}{}", config.api_base_url.trim_end_matches('/'), path);
        Ok(self
            .client
            .get(url)
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "gatebase")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn send_json<T>(&self, method: reqwest::Method, path: &str, body: &T) -> Result<()>
    where
        T: Serialize + Sync,
    {
        let config = self
            .config
            .as_ref()
            .context("GitHub App is not configured")?;
        let token = self.installation_token().await?;
        let url = format!("{}{}", config.api_base_url.trim_end_matches('/'), path);
        self.client
            .request(method, url)
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "gatebase")
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn installation_token(&self) -> Result<String> {
        let config = self
            .config
            .as_ref()
            .context("GitHub App is not configured")?;
        let jwt = github_app_jwt(config)?;
        let url = format!(
            "{}/app/installations/{}/access_tokens",
            config.api_base_url.trim_end_matches('/'),
            config.installation_id
        );
        let response: InstallationTokenResponse = self
            .client
            .post(url)
            .bearer_auth(jwt)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "gatebase")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.token)
    }
}

#[async_trait]
impl GitProvider for GitHubProvider {
    async fn issue(&self, repo: &str, issue: i64) -> Result<Issue> {
        self.get(&format!("/repos/{repo}/issues/{issue}")).await
    }

    async fn evaluate_signal(
        &self,
        request: &AccessRequest,
        signal: &AccessSignal,
    ) -> Result<SignalEvaluation> {
        let issue = self.issue(&request.github_repo, request.issue).await?;
        match signal {
            AccessSignal::GitHubIssueOpen => {
                if issue.state == "open" {
                    Ok(SignalEvaluation::allowed())
                } else {
                    Ok(SignalEvaluation::denied("issue is not open"))
                }
            }
            AccessSignal::GitHubIssueLabels { labels } => {
                let missing: Vec<&String> = labels
                    .iter()
                    .filter(|required| {
                        !issue
                            .labels
                            .iter()
                            .any(|label| label.name == required.as_str())
                    })
                    .collect();
                if missing.is_empty() {
                    Ok(SignalEvaluation::allowed())
                } else {
                    Ok(SignalEvaluation::denied(format!(
                        "required issue labels missing: {}",
                        missing
                            .into_iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )))
                }
            }
        }
    }

    async fn comment_issue(&self, repo: &str, issue: i64, body: &str) -> Result<()> {
        self.send_json(
            reqwest::Method::POST,
            &format!("/repos/{repo}/issues/{issue}/comments"),
            &IssueCommentRequest { body },
        )
        .await
    }

    async fn close_issue(&self, repo: &str, issue: i64) -> Result<()> {
        self.send_json(
            reqwest::Method::PATCH,
            &format!("/repos/{repo}/issues/{issue}"),
            &CloseIssueRequest { state: "closed" },
        )
        .await
    }
}

#[derive(Debug, Serialize)]
struct JwtClaims<'a> {
    iss: &'a str,
    iat: i64,
    exp: i64,
}

fn github_app_jwt(config: &GitHubAppConfig) -> Result<String> {
    let now = Utc::now();
    let claims = JwtClaims {
        iss: &config.app_id,
        iat: (now - Duration::seconds(60)).timestamp(),
        exp: (now + Duration::minutes(9)).timestamp(),
    };
    let key = EncodingKey::from_rsa_pem(&config.private_key_pem)?;
    let mut header = Header::new(Algorithm::RS256);
    header.typ = Some("JWT".to_owned());
    Ok(encode(&header, &claims, &key)?)
}

pub fn verify_webhook_signature(secret: &str, body: &[u8], header: &str) -> bool {
    let Some(signature) = header.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = hex::decode(signature) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

#[derive(Debug, Deserialize)]
struct InstallationTokenResponse {
    token: String,
    #[allow(dead_code)]
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Issue {
    pub number: i64,
    pub state: String,
    pub user: User,
    #[serde(default)]
    pub labels: Vec<IssueLabel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueLabel {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub login: String,
}

#[derive(Debug, Serialize)]
struct IssueCommentRequest<'a> {
    body: &'a str,
}

#[derive(Debug, Serialize)]
struct CloseIssueRequest<'a> {
    state: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_valid_webhook_signature() {
        let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
        mac.update(b"payload");
        let header = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify_webhook_signature("secret", b"payload", &header));
    }

    #[test]
    fn rejects_invalid_webhook_signature() {
        assert!(!verify_webhook_signature(
            "secret",
            b"payload",
            "sha256=deadbeef"
        ));
    }
}
