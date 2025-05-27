use std::fmt;

/// Error types for AnyQueue operations
#[derive(Debug)]
pub enum Error {
    /// Redis connection or operation errors
    Redis(redis::RedisError),
    /// JSON serialization/deserialization errors
    Serialization(serde_json::Error),
    /// Configuration errors
    Config(String),
    /// Job processing errors
    JobProcessing(String),
    /// Worker errors
    Worker(String),
    /// Connection errors
    Connection(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Redis(e) => write!(f, "Redis error: {}", e),
            Error::Serialization(e) => write!(f, "Serialization error: {}", e),
            Error::Config(msg) => write!(f, "Configuration error: {}", msg),
            Error::JobProcessing(msg) => write!(f, "Job processing error: {}", msg),
            Error::Worker(msg) => write!(f, "Worker error: {}", msg),
            Error::Connection(msg) => write!(f, "Connection error: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

impl From<redis::RedisError> for Error {
    fn from(err: redis::RedisError) -> Self {
        Error::Redis(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Serialization(err)
    }
}

/// Result type for AnyQueue operations
pub type Result<T> = std::result::Result<T, Error>;
