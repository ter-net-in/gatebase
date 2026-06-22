#[derive(Debug, thiserror::Error)]
pub enum GatebaseError {
    #[error("unsupported database engine: {0}")]
    UnsupportedEngine(String),
    #[error("invalid operation: {0}")]
    InvalidOperation(String),
}
