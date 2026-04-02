use thiserror::Error;

#[derive(Debug, Error)]
pub enum MastError {
    #[error("storage error: {0}")]
    Storage(String),

    #[error("index error: {0}")]
    Index(String),

    #[error("embedder error: {0}")]
    Embedder(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("compaction error: {0}")]
    Compaction(String),

    #[error("graph error: {0}")]
    Graph(String),

    #[error("snapshot error: {0}")]
    Snapshot(String),
}

impl From<redb::DatabaseError> for MastError {
    fn from(e: redb::DatabaseError) -> Self {
        MastError::Storage(e.to_string())
    }
}

impl From<redb::TableError> for MastError {
    fn from(e: redb::TableError) -> Self {
        MastError::Storage(e.to_string())
    }
}

impl From<redb::TransactionError> for MastError {
    fn from(e: redb::TransactionError) -> Self {
        MastError::Storage(e.to_string())
    }
}

impl From<redb::StorageError> for MastError {
    fn from(e: redb::StorageError) -> Self {
        MastError::Storage(e.to_string())
    }
}

impl From<redb::CommitError> for MastError {
    fn from(e: redb::CommitError) -> Self {
        MastError::Storage(e.to_string())
    }
}

impl From<bincode::Error> for MastError {
    fn from(e: bincode::Error) -> Self {
        MastError::Serialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let e = MastError::Storage("disk full".into());
        assert_eq!(e.to_string(), "storage error: disk full");

        let e = MastError::Index("corrupt".into());
        assert_eq!(e.to_string(), "index error: corrupt");

        let e = MastError::Embedder("timeout".into());
        assert_eq!(e.to_string(), "embedder error: timeout");

        let e = MastError::NotFound("id 123".into());
        assert_eq!(e.to_string(), "not found: id 123");

        let e = MastError::InvalidInput("bad query".into());
        assert_eq!(e.to_string(), "invalid input: bad query");

        let e = MastError::Serialization("decode fail".into());
        assert_eq!(e.to_string(), "serialization error: decode fail");
    }

    #[test]
    fn error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MastError>();
    }
}
