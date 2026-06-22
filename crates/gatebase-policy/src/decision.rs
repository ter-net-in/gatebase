use crate::classifier::classify;
use crate::operation::operation_name;
use crate::rules::{blocks_by_default, contains_where, requires_where, risk};
use gatebase_config::PolicyConfig;
use gatebase_core::{Decision, RiskLevel};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub decision: Decision,
    pub risk: RiskLevel,
    pub reason: Option<String>,
}

pub fn decide(statement: &str, policy: &PolicyConfig) -> PolicyDecision {
    let op = classify(statement);
    let normalized = operation_name(op);

    if blocks_by_default(op) || policy.block.iter().any(|blocked| blocked == normalized) {
        return PolicyDecision {
            decision: Decision::Blocked,
            risk: risk(op),
            reason: Some(format!("operation {normalized} is blocked by policy")),
        };
    }

    if requires_where(op, policy) && !contains_where(statement) {
        return PolicyDecision {
            decision: Decision::Blocked,
            risk: risk(op),
            reason: Some(format!("operation {normalized} requires WHERE clause")),
        };
    }

    PolicyDecision {
        decision: Decision::Allowed,
        risk: risk(op),
        reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_drop_database() {
        let decision = decide("DROP DATABASE prod", &PolicyConfig::default());
        assert_eq!(decision.decision, Decision::Blocked);
    }

    #[test]
    fn blocks_multi_statement_by_default() {
        let decision = decide("SELECT 1; SELECT 2", &PolicyConfig::default());
        assert_eq!(decision.decision, Decision::Blocked);
    }

    #[test]
    fn ignores_semicolon_inside_string_literal() {
        let decision = decide("SELECT ';'", &PolicyConfig::default());
        assert_eq!(decision.decision, Decision::Allowed);
    }

    #[test]
    fn ignores_semicolon_inside_comments() {
        let decision = decide(
            "SELECT 1 -- ; SELECT 2\n/* ; SELECT 3 */",
            &PolicyConfig::default(),
        );
        assert_eq!(decision.decision, Decision::Allowed);
    }

    #[test]
    fn allows_single_statement_with_trailing_semicolon() {
        let decision = decide("SELECT 1;", &PolicyConfig::default());
        assert_eq!(decision.decision, Decision::Allowed);
    }

    #[test]
    fn blocks_copy_program_by_default() {
        let decision = decide("COPY users TO PROGRAM 'cat'", &PolicyConfig::default());
        assert_eq!(decision.decision, Decision::Blocked);
    }

    #[test]
    fn blocks_create_extension_by_default() {
        let decision = decide("CREATE EXTENSION pgcrypto", &PolicyConfig::default());
        assert_eq!(decision.decision, Decision::Blocked);
    }

    #[test]
    fn blocks_security_definer_by_default() {
        let decision = decide(
            "CREATE FUNCTION f() RETURNS void SECURITY DEFINER LANGUAGE sql AS $$ SELECT 1 $$",
            &PolicyConfig::default(),
        );
        assert_eq!(decision.decision, Decision::Blocked);
    }

    #[test]
    fn blocks_set_global_by_default() {
        let decision = decide("SET GLOBAL read_only = 0", &PolicyConfig::default());
        assert_eq!(decision.decision, Decision::Blocked);
    }

    #[test]
    fn blocks_load_data_by_default() {
        let decision = decide(
            "LOAD DATA LOCAL INFILE '/tmp/users.csv' INTO TABLE users",
            &PolicyConfig::default(),
        );
        assert_eq!(decision.decision, Decision::Blocked);
    }

    #[test]
    fn requires_where_for_update() {
        let policy = PolicyConfig {
            require_where: vec!["update".to_owned()],
            ..PolicyConfig::default()
        };
        let decision = decide("UPDATE users SET admin = true", &policy);
        assert_eq!(decision.decision, Decision::Blocked);
    }

    #[test]
    fn ignores_where_inside_string_literal() {
        let policy = PolicyConfig {
            require_where: vec!["update".to_owned()],
            ..PolicyConfig::default()
        };
        let decision = decide("UPDATE users SET note = 'where'", &policy);
        assert_eq!(decision.decision, Decision::Blocked);
    }

    #[test]
    fn allows_update_with_real_where() {
        let policy = PolicyConfig {
            require_where: vec!["update".to_owned()],
            ..PolicyConfig::default()
        };
        let decision = decide("UPDATE users SET admin = true WHERE id = 1", &policy);
        assert_eq!(decision.decision, Decision::Allowed);
    }
}
