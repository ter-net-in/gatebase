use crate::GatebaseError;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DbEngine {
    Postgres,
    Mysql,
}

impl Display for DbEngine {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Postgres => f.write_str("postgres"),
            Self::Mysql => f.write_str("mysql"),
        }
    }
}

impl FromStr for DbEngine {
    type Err = GatebaseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "postgres" | "postgresql" => Ok(Self::Postgres),
            "mysql" | "mariadb" => Ok(Self::Mysql),
            other => Err(GatebaseError::UnsupportedEngine(other.to_owned())),
        }
    }
}
