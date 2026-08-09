use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to spawn claude: {0}")]
    Spawn(String),
    #[error("session not found")]
    SessionNotFound,
    #[error("session is not running")]
    SessionGone,
    #[error("{0}")]
    Control(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
