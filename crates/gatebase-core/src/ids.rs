use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SessionId(String);

impl SessionId {
    #[must_use]
    pub fn new() -> Self {
        Self(format!("sess_{}", Uuid::new_v4().simple()))
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for SessionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for SessionId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AuditEventId(String);

impl AuditEventId {
    #[must_use]
    pub fn new() -> Self {
        Self(format!("evt_{}", Uuid::new_v4().simple()))
    }
}

impl Default for AuditEventId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for AuditEventId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for AuditEventId {
    fn from(value: String) -> Self {
        Self(value)
    }
}
