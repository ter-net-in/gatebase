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
    async fn validate_access_request(&self, request: &AccessRequest) -> Result<bool>;
    async fn evaluate_signal(
        &self,
        request: &AccessRequest,
        signal: &AccessSignal,
    ) -> Result<SignalEvaluation>;
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

    async fn pull_request(&self, request: &AccessRequest) -> Result<PullRequest> {
        let number = request
            .pull_request
            .context("GitHub access signals require a pull request")?;
        self.get(&format!("/repos/{}/pulls/{number}", request.repo))
            .await
    }

    async fn reviews(&self, request: &AccessRequest) -> Result<Vec<Review>> {
        let number = request
            .pull_request
            .context("GitHub access signals require a pull request")?;
        self.get(&format!("/repos/{}/pulls/{number}/reviews", request.repo))
            .await
    }

    async fn checks(&self, request: &AccessRequest, sha: &str) -> Result<Vec<NamedCheck>> {
        let check_runs: CheckRunsResponse = self
            .get(&format!("/repos/{}/commits/{sha}/check-runs", request.repo))
            .await?;
        let combined_status: CombinedStatus = self
            .get(&format!("/repos/{}/commits/{sha}/status", request.repo))
            .await?;

        Ok(check_runs
            .check_runs
            .into_iter()
            .map(|run| NamedCheck {
                name: run.name,
                passed: run.status == "completed" && run.conclusion.as_deref() == Some("success"),
            })
            .chain(
                combined_status
                    .statuses
                    .into_iter()
                    .map(|status| NamedCheck {
                        name: status.context,
                        passed: status.state == "success",
                    }),
            )
            .collect())
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
        let response = self
            .client
            .get(url)
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "gatebase")
            .send()
            .await?;
        let response = response.error_for_status()?;
        Ok(response.json().await?)
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
    async fn validate_access_request(&self, request: &AccessRequest) -> Result<bool> {
        if request.pull_request.is_none() {
            return Ok(false);
        }
        Ok(self.pull_request(request).await.is_ok())
    }

    async fn evaluate_signal(
        &self,
        request: &AccessRequest,
        signal: &AccessSignal,
    ) -> Result<SignalEvaluation> {
        match signal {
            AccessSignal::ManualApproval { .. } | AccessSignal::CliApproval { .. } => Ok(
                SignalEvaluation::denied("manual approval signals require an approval provider"),
            ),
            AccessSignal::GitHubPullRequestOpen => {
                let pull = self.pull_request(request).await?;
                if pull.state == "open" {
                    Ok(SignalEvaluation::allowed())
                } else {
                    Ok(SignalEvaluation::denied("pull request is not open"))
                }
            }
            AccessSignal::GitHubPullRequestApproved => {
                let reviews = self.reviews(request).await?;
                if has_current_approval(&reviews) {
                    Ok(SignalEvaluation::allowed())
                } else {
                    Ok(SignalEvaluation::denied(
                        "pull request has no current approval",
                    ))
                }
            }
            AccessSignal::GitHubChecksPassed { checks } => {
                let pull = self.pull_request(request).await?;
                let actual = self.checks(request, &pull.head.sha).await?;
                let missing: Vec<&String> = checks
                    .iter()
                    .filter(|required| {
                        !actual
                            .iter()
                            .any(|check| check.name == required.as_str() && check.passed)
                    })
                    .collect();
                if missing.is_empty() {
                    Ok(SignalEvaluation::allowed())
                } else {
                    Ok(SignalEvaluation::denied(format!(
                        "required checks not successful: {}",
                        missing
                            .into_iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )))
                }
            }
            AccessSignal::GitHubLabels { labels } => {
                let pull = self.pull_request(request).await?;
                let missing: Vec<&String> = labels
                    .iter()
                    .filter(|required| {
                        !pull
                            .labels
                            .iter()
                            .any(|label| label.name == required.as_str())
                    })
                    .collect();
                if missing.is_empty() {
                    Ok(SignalEvaluation::allowed())
                } else {
                    Ok(SignalEvaluation::denied(format!(
                        "required labels missing: {}",
                        missing
                            .into_iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )))
                }
            }
            AccessSignal::GitHubCodeownersReviewed => {
                let pull = self.pull_request(request).await?;
                let reviews = self.reviews(request).await?;
                if !pull.requested_reviewers.is_empty() || !pull.requested_teams.is_empty() {
                    return Ok(SignalEvaluation::denied(
                        "pull request still has requested reviewers or teams",
                    ));
                }
                if has_current_approval(&reviews) {
                    Ok(SignalEvaluation::allowed())
                } else {
                    Ok(SignalEvaluation::denied(
                        "CODEOWNERS review approval not found",
                    ))
                }
            }
        }
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

fn has_current_approval(reviews: &[Review]) -> bool {
    let mut approvals = std::collections::HashMap::new();
    for review in reviews {
        approvals.insert(review.user.login.as_str(), review.state.as_str());
    }
    approvals.values().any(|state| *state == "APPROVED")
}

#[derive(Debug, Deserialize)]
struct InstallationTokenResponse {
    token: String,
    #[allow(dead_code)]
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct PullRequest {
    state: String,
    head: PullRequestHead,
    #[serde(default)]
    labels: Vec<Label>,
    #[serde(default)]
    requested_reviewers: Vec<User>,
    #[serde(default)]
    requested_teams: Vec<Team>,
}

#[derive(Debug, Deserialize)]
struct PullRequestHead {
    sha: String,
}

#[derive(Debug, Deserialize)]
struct Label {
    name: String,
}

#[derive(Debug, Deserialize)]
struct User {
    login: String,
}

#[derive(Debug, Deserialize)]
struct Team {}

#[derive(Debug, Deserialize)]
struct Review {
    user: User,
    state: String,
}

#[derive(Debug)]
struct NamedCheck {
    name: String,
    passed: bool,
}

#[derive(Debug, Deserialize)]
struct CheckRunsResponse {
    #[serde(default)]
    check_runs: Vec<CheckRun>,
}

#[derive(Debug, Deserialize)]
struct CheckRun {
    name: String,
    status: String,
    conclusion: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CombinedStatus {
    #[serde(default)]
    statuses: Vec<CommitStatus>,
}

#[derive(Debug, Deserialize)]
struct CommitStatus {
    context: String,
    state: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    #[test]
    fn latest_review_state_per_user_controls_approval() {
        let reviews = vec![
            Review {
                user: User {
                    login: "alice".to_owned(),
                },
                state: "APPROVED".to_owned(),
            },
            Review {
                user: User {
                    login: "alice".to_owned(),
                },
                state: "CHANGES_REQUESTED".to_owned(),
            },
        ];
        assert!(!has_current_approval(&reviews));
    }

    #[tokio::test]
    async fn evaluates_full_github_signal_set() {
        let server = MockServer::start().await;
        mock_token(&server).await;
        mock_pull(&server, "open", &["db-access-approved"]).await;
        mock_reviews(&server, &[("alice", "APPROVED")]).await;
        mock_check_runs(&server, &[("ci", "completed", Some("success"))]).await;
        mock_statuses(&server, &[("legacy-ci", "success")]).await;

        let provider = test_provider(server.uri());
        let request = test_request();
        for signal in [
            AccessSignal::GitHubPullRequestOpen,
            AccessSignal::GitHubPullRequestApproved,
            AccessSignal::GitHubChecksPassed {
                checks: vec!["ci".to_owned(), "legacy-ci".to_owned()],
            },
            AccessSignal::GitHubLabels {
                labels: vec!["db-access-approved".to_owned()],
            },
            AccessSignal::GitHubCodeownersReviewed,
        ] {
            let evaluation = provider.evaluate_signal(&request, &signal).await.unwrap();
            assert_eq!(evaluation, SignalEvaluation::allowed());
        }
    }

    #[tokio::test]
    async fn denies_missing_required_check() {
        let server = MockServer::start().await;
        mock_token(&server).await;
        mock_pull(&server, "open", &[]).await;
        mock_check_runs(&server, &[("ci", "completed", Some("failure"))]).await;
        mock_statuses(&server, &[]).await;

        let provider = test_provider(server.uri());
        let evaluation = provider
            .evaluate_signal(
                &test_request(),
                &AccessSignal::GitHubChecksPassed {
                    checks: vec!["ci".to_owned()],
                },
            )
            .await
            .unwrap();
        assert_eq!(
            evaluation,
            SignalEvaluation::denied("required checks not successful: ci")
        );
    }

    fn test_provider(api_base_url: String) -> GitHubProvider {
        GitHubProvider::new(GitHubAppConfig {
            app_id: "123".to_owned(),
            installation_id: 456,
            private_key_pem: TEST_PRIVATE_KEY.as_bytes().to_vec(),
            webhook_secret: "secret".to_owned(),
            api_base_url,
        })
    }

    fn test_request() -> AccessRequest {
        AccessRequest {
            actor: "alice".to_owned(),
            repo: "gatebase/gatebase".to_owned(),
            pull_request: Some(7),
            target: "prod-pg".to_owned(),
        }
    }

    async fn mock_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/app/installations/456/access_tokens"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "token": "installation-token",
                "expires_at": "2030-01-01T00:00:00Z"
            })))
            .mount(server)
            .await;
    }

    async fn mock_pull(server: &MockServer, state: &str, labels: &[&str]) {
        let labels: Vec<_> = labels
            .iter()
            .map(|name| serde_json::json!({ "name": name }))
            .collect();
        Mock::given(method("GET"))
            .and(path("/repos/gatebase/gatebase/pulls/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "state": state,
                "head": { "sha": "abc123" },
                "labels": labels,
                "requested_reviewers": [],
                "requested_teams": []
            })))
            .mount(server)
            .await;
    }

    async fn mock_reviews(server: &MockServer, reviews: &[(&str, &str)]) {
        let reviews: Vec<_> = reviews
            .iter()
            .map(|(login, state)| serde_json::json!({ "user": { "login": login }, "state": state }))
            .collect();
        Mock::given(method("GET"))
            .and(path("/repos/gatebase/gatebase/pulls/7/reviews"))
            .respond_with(ResponseTemplate::new(200).set_body_json(reviews))
            .mount(server)
            .await;
    }

    async fn mock_check_runs(server: &MockServer, runs: &[(&str, &str, Option<&str>)]) {
        let runs: Vec<_> = runs
            .iter()
            .map(|(name, status, conclusion)| {
                serde_json::json!({ "name": name, "status": status, "conclusion": conclusion })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/repos/gatebase/gatebase/commits/abc123/check-runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "check_runs": runs
            })))
            .mount(server)
            .await;
    }

    async fn mock_statuses(server: &MockServer, statuses: &[(&str, &str)]) {
        let statuses: Vec<_> = statuses
            .iter()
            .map(|(context, state)| serde_json::json!({ "context": context, "state": state }))
            .collect();
        Mock::given(method("GET"))
            .and(path("/repos/gatebase/gatebase/commits/abc123/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "statuses": statuses
            })))
            .mount(server)
            .await;
    }

    const TEST_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC9d2tdoBnznHVx\ncfXKrBYLWn2imMvZfkV6jiadqGL4pgyXw1UdK3frSPZ3XitsXjB9+nU4KdFNvLJh\njzGjn+2zprISS/6c62Ws7qxBsqpYkhfC5kmj9xDCu1/qEGOjlDyvVohxRu6HWUcj\nTyTRuUMU7Ovc7SEC68eZISBh0WnyKo5NpwLbL7Ov/KcwNP6Y3YgCUBnBYwcgCYhg\n7TWNdDuqJFikjMtBSosiYeUUjkt4a4pIYrRGTsD3AWEhGj8/w1ig/icO8kVMiSsQ\niDEcg1lhREw/3rNA8wma1UaArqwRHM2o0loLLoG8K2rzAqGLmY/AlrXeGxKnGzBu\ntX3T5J5tAgMBAAECggEAGY+GVxO13JVDjOIEeGq98JstLuXxPm7YHcAWupdQft4b\n/c926KAIJiBqS9FTB2Qvo5dKACE4IQYvhJTma/4z+dQn2usQtwfU6D8s0xIxBBD1\njrA1yB6ZTsQrnO7IGnXxt+/zKWEZ5f2n6L4RKBAX2jdaXKxLE1NO0rxS249+fRKA\nM2zk5bwZ+TRa2QUtxh1tvNBR0ZxTJ0yZikH7Hsrd3lNktvd3qPnB7U3XRm/qFrwn\nlsit8pfxRE7e5ysyQIEvHJTg+5sZuUuohosR2rwqkngF4Gehy2p99RDQfQuwa2tV\nBG740KK1/d5B8ih6gL2PPRTMliuHLJPG94n8gWhluwKBgQDmUwxRW9vqTstcEw2w\nwSoKB4fYVMogqnkffGP2lrGhcU+FjnV7ibgDWWR5cloPHuN9UIN797mTrxIEMaIJ\nEALBOwhmIoRC69+74WDwZ4DbSy/x936jYJN5pJF+pULIRFTYsLnqh3WtztQnVtRO\n83Mzda01xnFtxCgadT9/R+tOVwKBgQDSlmBBDKObmA0zRglMgI6ffL4EMBBjIhTS\nm/YeLf2xkXBHhA3w5KgeY7MmdB4g2m3hidohuqh2605XefSWYa+bNmrMvMS30rds\nqMmwkDyyRcdwm86W6xBJ+kRfaIZ/+gPWHE2eGVfImonlbInqQ/LhWSvJWQ2DH2hd\nPIj4/Zf22wKBgEWsX7CI2iix90B+HwkWFdJ83rIpTA+/oe5NYavpgAh3T3f2VUGu\nsnSI6ST325hxXp286llo4cF0Fz4fiYW2Sy8K3YqP7HSWB9M85Wcz2D3+K53FoZBo\nmZQVnVGzSlVsnkICi+sPWSDfzTutP4I2kOXDNkdrJUrwKLWAPFoTdnH3AoGAJPV7\nYy9Cr5YaCvupuiF95oPQHZAJ8DwVB3mT0mwj8DwkRojooXSgBOVelcsfVoi/bCvz\neqP12RknILcotBPk7Aq657/hjpmO06Uz8Kb/4BHbFOpjcZ1DuJgR0+TWZjOM5NEG\n1k6AV5a3yOopslHGmMI7qxTUCEVE3cg4CesH9q0CgYEAoR1I+zZUxNzJ0DCrhPr+\nvPhadALoI826Foo5yUidA9tkcwMXbQ+QfllRojf1oIITr9gC2nnPn1oSlgyVr13L\njvm4HbVDtJjeTribq2UY0dI/W+wrNrTNt4kWQCFTgxEn7u/j6DIshe4SnRBB0rr+\nw4+5pVtqcNkagERUo6tt9lk=\n-----END PRIVATE KEY-----\n";
}
