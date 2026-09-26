#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("corrupt presence value in devices table: {0:?}")]
    CorruptPresence(String),
    #[error("corrupt sighting source value in devices table: {0:?}")]
    CorruptSightingSource(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;
