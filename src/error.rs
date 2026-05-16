use thiserror::Error;

#[derive(Debug, Error)]
pub enum BTreeError {
    #[error("Key not found")]
    KeyNotFound,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),

    #[error("Corrupt page: page {page_id} failed integrity check")]
    CorruptPage { page_id: u64 },

    #[error("Pager is full: cannot allocate page (max {max} pages)")]
    PagerFull { max: u64 },

    #[error("Invalid tree order: t must be >= 2, got {got}")]
    InvalidOrder { got: usize },
}

pub type Result<T> = std::result::Result<T, BTreeError>;