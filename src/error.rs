use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApixError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Git error: {0}")]
    Git(String),
    #[error("Config error: {0}")]
    Config(String),
    #[error("Vault not found: {0}")]
    VaultNotFound(String),
    #[error("Route not found: {0}")]
    RouteNotFound(String),
    #[error("Ambiguous route/source: {0}")]
    Ambiguous(String),
}
