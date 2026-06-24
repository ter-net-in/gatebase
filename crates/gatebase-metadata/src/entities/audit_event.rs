use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "audit_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub session_id: String,
    pub actor: String,
    pub target: String,
    pub engine: String,
    pub statement: String,
    pub decision: String,
    pub rows_affected: Option<i64>,
    pub error: Option<String>,
    pub created_at: String,
    pub rollback_artifact_id: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
