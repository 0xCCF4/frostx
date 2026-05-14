use thiserror::Error;

/// Top-level frostx error type.
#[derive(Debug, Error)]
pub enum FrostxError {
    #[error("no frostx.toml found in {0}")]
    NotInitialized(std::path::PathBuf),

    #[error("UUID collision: current path {current} differs from state-recorded path {recorded}. Run `frostx init --force` to assign a new UUID.")]
    UuidCollision {
        current: std::path::PathBuf,
        recorded: std::path::PathBuf,
    },

    #[error("frostx.toml already exists; pass --force to overwrite")]
    AlreadyInitialized,

    #[error("{0}")]
    Config(String),

    #[error("include error in {path}: {message}")]
    Include { path: String, message: String },

    #[error("{0}")]
    UnknownAction(String),

    #[error("action '{action}' failed: {message}")]
    ActionFailed { action: String, message: String },

    #[error("backup config missing: add [config.backup] server = \"...\" to frostx.toml")]
    BackupConfigMissing,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// Exit code constants matching the CLI spec.
pub mod exit_code {
    /// Success.
    pub const OK: i32 = 0;
    /// General error (invalid config, action failure, etc.).
    pub const ERROR: i32 = 1;
    /// Warnings only (used by `doctor`).
    pub const WARNING: i32 = 2;
    /// Project not initialized - no `frostx.toml` found.
    pub const NOT_INITIALIZED: i32 = 3;
    /// UUID collision - project is a copy of another tracked project.
    pub const UUID_COLLISION: i32 = 4;
}

impl FrostxError {
    /// Maps each error variant to its documented exit code.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::NotInitialized(_) => exit_code::NOT_INITIALIZED,
            Self::UuidCollision { .. } => exit_code::UUID_COLLISION,
            _ => exit_code::ERROR,
        }
    }
}
