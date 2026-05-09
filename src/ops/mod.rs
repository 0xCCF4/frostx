//! High-level frostx operations.
//!
//! Each submodule composes config loading, scanning, pipeline evaluation, and
//! state management into a single callable unit. The `frostx` binary is a thin
//! adapter over these functions; other applications can call them directly.

/// `frostx check` - inspect a project without running actions.
pub mod check;
/// `frostx doctor` - validate `frostx.toml` for correctness.
pub mod doctor;
/// `frostx gc` - remove orphaned state files.
pub mod gc;
/// `frostx init` - create `frostx.toml` with a generated UUID.
pub mod init;
/// `frostx projects` - manage the tracked-project registry.
pub mod projects;
/// `frostx run` - execute the inactivity pipeline.
pub mod run;
/// `frostx scan` - walk a directory tree and report all managed projects.
pub mod scan;

use std::path::PathBuf;

/// Shared context passed to all operation functions.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct FrostxOpts {
    /// Show what would happen without executing any actions.
    pub dry_run: bool,
    /// Increase output verbosity.
    pub verbose: bool,
    /// Suppress all output except errors.
    pub quiet: bool,
    /// Skip interactive confirmations.
    pub yes: bool,
    /// Override the `frostx.toml` path.
    pub config_override: Option<PathBuf>,
    /// Config library directory.
    pub library_dir: PathBuf,
    /// State directory (`$XDG_DATA_HOME/frostx/`).
    pub state_dir: PathBuf,
}
