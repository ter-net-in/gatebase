#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SqlOperation {
    MultiStatement,
    Select,
    Insert,
    Update,
    Delete,
    DropDatabase,
    DropTable,
    Truncate,
    AlterSystem,
    CopyProgram,
    CreateExtension,
    SecurityDefiner,
    SetGlobal,
    LoadData,
    Other,
}

pub(crate) fn operation_name(op: SqlOperation) -> &'static str {
    match op {
        SqlOperation::MultiStatement => "multi_statement",
        SqlOperation::Select => "select",
        SqlOperation::Insert => "insert",
        SqlOperation::Update => "update",
        SqlOperation::Delete => "delete",
        SqlOperation::DropDatabase => "drop_database",
        SqlOperation::DropTable => "drop_table",
        SqlOperation::Truncate => "truncate",
        SqlOperation::AlterSystem => "alter_system",
        SqlOperation::CopyProgram => "copy_program",
        SqlOperation::CreateExtension => "create_extension",
        SqlOperation::SecurityDefiner => "security_definer",
        SqlOperation::SetGlobal => "set_global",
        SqlOperation::LoadData => "load_data",
        SqlOperation::Other => "other",
    }
}
