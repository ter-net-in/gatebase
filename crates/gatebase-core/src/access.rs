use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AccessSignal {
    #[serde(rename = "github_issue_open")]
    GitHubIssueOpen,
    #[serde(rename = "github_issue_labels")]
    GitHubIssueLabels { labels: Vec<String> },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccessToken {
    pub id: String,
    pub token_hash: String,
    pub actor: String,
    pub github_repo: String,
    pub issue: i64,
    pub target: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}
