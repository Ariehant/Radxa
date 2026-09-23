use serde::Serialize;

pub type Result<T, E = NexusError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum NexusError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no vault is open")]
    NoVault,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0} changed on disk")]
    Conflict(String),
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("{0}")]
    Other(String),
}

impl NexusError {
    pub fn invalid(msg: impl Into<String>) -> Self {
        NexusError::Invalid(msg.into())
    }

    /// JSON-RPC 2.0 error code. -32000..-32099 is reserved for implementation errors.
    pub fn code(&self) -> i64 {
        match self {
            NexusError::Invalid(_) | NexusError::Yaml(_) | NexusError::Json(_) => -32602,
            NexusError::NotFound(_) => -32004,
            NexusError::NoVault => -32001,
            NexusError::Conflict(_) => -32009,
            _ => -32000,
        }
    }
}

impl Serialize for NexusError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
