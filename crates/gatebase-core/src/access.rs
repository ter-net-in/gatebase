use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AccessSignal {
    #[serde(rename = "github_pull_request_open")]
    GitHubPullRequestOpen,
    #[serde(rename = "github_pull_request_approved")]
    GitHubPullRequestApproved,
    #[serde(rename = "github_checks_passed")]
    GitHubChecksPassed {
        checks: Vec<String>,
    },
    #[serde(rename = "github_labels")]
    GitHubLabels {
        labels: Vec<String>,
    },
    #[serde(rename = "github_codeowners_reviewed")]
    GitHubCodeownersReviewed,
    ManualApproval {
        approvers: Vec<String>,
    },
    CliApproval {
        approvers: Vec<String>,
        #[serde(default)]
        allow_without_pull_request: bool,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccessApproval {
    pub id: String,
    pub repo: String,
    pub pull_request: Option<i64>,
    pub target: String,
    pub actor: Option<String>,
    pub approver: String,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}
