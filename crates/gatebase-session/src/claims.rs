use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Claims {
    pub(crate) sub: String,
    pub(crate) session_id: String,
    pub(crate) source_type: String,
    pub(crate) github_repo: Option<String>,
    pub(crate) issue: Option<i64>,
    pub(crate) target: String,
    pub(crate) scopes: Vec<String>,
    pub(crate) exp: usize,
}
