#[derive(Debug, Clone)]
pub struct AccessRequest {
    pub github_repo: String,
    pub issue: i64,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalEvaluation {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl SignalEvaluation {
    #[must_use]
    pub fn allowed() -> Self {
        Self {
            allowed: true,
            reason: None,
        }
    }

    #[must_use]
    pub fn denied(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
        }
    }
}
