use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("raw I/O error: {0}")]
    RawIo(#[from] std::io::Error),

    #[error("JSON error in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("raw JSON error: {0}")]
    RawJson(#[from] serde_json::Error),

    #[error("TOML (de)serialization error: {0}")]
    Toml(String),

    #[error("SQLite error: {0}")]
    Sql(#[from] rusqlite::Error),

    #[error("unknown agent: {0}")]
    UnknownAgent(String),

    #[error("profile not found: agent={agent} id={id}")]
    ProfileNotFound { agent: String, id: String },

    #[error("skill not found: {0}")]
    SkillNotFound(String),

    #[error("no current profile for agent {0}")]
    NoCurrentProfile(String),

    #[error("secret ref not resolvable: {0}")]
    UnresolvedSecret(String),

    #[error("schema migration failed: {0}")]
    Migration(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<toml::de::Error> for Error {
    fn from(value: toml::de::Error) -> Self {
        Error::Toml(value.to_string())
    }
}

impl From<toml::ser::Error> for Error {
    fn from(value: toml::ser::Error) -> Self {
        Error::Toml(value.to_string())
    }
}
