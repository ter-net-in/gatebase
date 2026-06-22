use crate::classifier::has_token;
use crate::operation::{operation_name, SqlOperation};
use gatebase_config::PolicyConfig;
use gatebase_core::RiskLevel;

pub(crate) fn risk(op: SqlOperation) -> RiskLevel {
    match op {
        SqlOperation::Select => RiskLevel::Low,
        SqlOperation::Insert => RiskLevel::Medium,
        SqlOperation::Update | SqlOperation::Delete => RiskLevel::High,
        SqlOperation::MultiStatement
        | SqlOperation::DropDatabase
        | SqlOperation::DropTable
        | SqlOperation::Truncate
        | SqlOperation::AlterSystem
        | SqlOperation::CopyProgram
        | SqlOperation::CreateExtension
        | SqlOperation::SecurityDefiner
        | SqlOperation::SetGlobal
        | SqlOperation::LoadData => RiskLevel::Critical,
        SqlOperation::Other => RiskLevel::Medium,
    }
}

pub(crate) fn blocks_by_default(op: SqlOperation) -> bool {
    matches!(
        op,
        SqlOperation::MultiStatement
            | SqlOperation::DropDatabase
            | SqlOperation::DropTable
            | SqlOperation::Truncate
            | SqlOperation::AlterSystem
            | SqlOperation::CopyProgram
            | SqlOperation::CreateExtension
            | SqlOperation::SecurityDefiner
            | SqlOperation::SetGlobal
            | SqlOperation::LoadData
    )
}

pub(crate) fn requires_where(op: SqlOperation, policy: &PolicyConfig) -> bool {
    policy
        .require_where
        .iter()
        .any(|required| required == operation_name(op))
}

pub(crate) fn contains_where(statement: &str) -> bool {
    has_token(statement, "where")
}
