use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "rollback_artifacts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub session_id: String,
    pub actor: String,
    pub target: String,
    pub engine: String,
    pub statement: String,
    pub table_name: Option<String>,
    pub primary_key_column: Option<String>,
    pub before_rows: String,
    pub inverse_sql: Option<String>,
    pub manual_required: bool,
    pub reason: Option<String>,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
