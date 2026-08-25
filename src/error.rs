use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShellError {
    #[error("Syntax error: {0}")]
    Syntax(String),
    #[error("Execution error: {0}")]
    Execution(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("System error: {0}")]
    Nix(#[from] nix::Error),
    #[error("Command not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, ShellError>;
