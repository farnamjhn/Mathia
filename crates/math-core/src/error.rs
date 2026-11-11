use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to render the HTML template: {0}")]
    Template(#[from] askama::Error),
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The scoring configuration could not be parsed or failed validation.
    #[error("invalid configuration: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, Error>;
