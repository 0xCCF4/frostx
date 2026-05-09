//! Built-in action implementations and the shared `Action` trait.
//!
//! Each submodule implements one category of actions. New categories are added
//! by creating a submodule and registering the new name(s) in [`create`].

/// Action implementations for creating compressed archives.
pub mod archive;
/// Action implementations for backup server interaction.
pub mod backup;
/// Action implementations for filesystem cleanup.
pub mod fs;
/// Action implementations for git operations.
pub mod git;
/// Action implementation for user-defined shell hooks.
pub mod hook;
/// Action implementations for Jujutsu (jj) VCS operations.
pub mod jj;
/// Action implementation for local project deletion.
pub mod local;
/// Action implementation for user notifications requiring explicit confirmation.
pub mod notify;
/// VCS-agnostic action wrappers that auto-detect git or jj.
pub mod vcs;

use crate::config::project::ProjectConfig;
use crate::error::FrostxError;
use std::path::Path;

/// Whether an action is a check or a mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    /// Read-only assertion. Re-evaluated on every run; never recorded as completed.
    Check,
    /// Performs a change. Recorded as completed; skipped on re-runs unless `--force`.
    Mutation,
}

/// Outcome of executing a single action.
#[derive(Debug, Clone)]
pub struct ActionOutcome {
    pub status: crate::pipeline::ActionStatus,
    pub message: String,
}

impl ActionOutcome {
    /// Construct a successful outcome.
    pub fn ok(msg: impl Into<String>) -> Self {
        Self {
            status: crate::pipeline::ActionStatus::Ok,
            message: msg.into(),
        }
    }

    /// Construct a failed outcome.
    pub fn failed(msg: impl Into<String>) -> Self {
        Self {
            status: crate::pipeline::ActionStatus::Failed,
            message: msg.into(),
        }
    }

    /// Construct a skipped outcome (preceding action failed).
    pub fn skipped(msg: impl Into<String>) -> Self {
        Self {
            status: crate::pipeline::ActionStatus::Skipped,
            message: msg.into(),
        }
    }

    /// Construct a dry-run outcome (action was suppressed by `--dry-run`).
    pub fn dry_run(msg: impl Into<String>) -> Self {
        Self {
            status: crate::pipeline::ActionStatus::DryRun,
            message: msg.into(),
        }
    }
}

/// Context passed to every action execution.
pub struct ActionContext<'a> {
    pub project_path: &'a Path,
    pub config: &'a ProjectConfig,
    pub dry_run: bool,
    pub yes: bool,
}

/// The core action trait. Implement this to add new actions.
pub trait Action: Send + Sync {
    /// Stable dot-separated name, e.g. `"git.check_clean"`.
    #[allow(dead_code)]
    fn name(&self) -> &'static str;

    /// Whether this action is a check or mutation.
    fn kind(&self) -> ActionKind;

    /// Execute the action and return an outcome.
    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError>;
}

/// Create a boxed [`Action`] from its name string.
///
/// This is the single registration point - adding a new action requires only
/// adding a new `match` arm here and implementing the `Action` trait.
///
/// Static actions are registered below to the array as well.
pub fn create(name: &str, config: &ProjectConfig) -> Result<Box<dyn Action>, FrostxError> {
    match name {
        "git.check_clean" => Ok(Box::new(git::CheckClean)),
        "git.check_pushed" => Ok(Box::new(git::CheckPushed)),
        "git.clean" => Ok(Box::new(git::Clean)),
        "git.tag" => Ok(Box::new(git::Tag)),
        "jj.check_clean" => Ok(Box::new(jj::CheckClean)),
        "jj.check_pushed" => Ok(Box::new(jj::CheckPushed)),
        "jj.bookmark" => Ok(Box::new(jj::Bookmark)),
        "vcs.check_clean" => Ok(Box::new(vcs::CheckClean)),
        "vcs.check_pushed" => Ok(Box::new(vcs::CheckPushed)),
        "vcs.mark" => Ok(Box::new(vcs::Mark)),
        "fs.clean_artifacts" => Ok(Box::new(fs::CleanArtifacts::new(config))),
        "archive.tar_gz" => Ok(Box::new(archive::TarGz::new(config))),
        "backup.check" => Ok(Box::new(backup::Check::new(config)?)),
        "backup.upload" => Ok(Box::new(backup::Upload::new(config)?)),
        "backup.verify" => Ok(Box::new(backup::Verify::new(config)?)),
        "local.delete" => Ok(Box::new(local::Delete)),
        name if name.starts_with("notify.") => {
            let notify_name = &name["notify.".len()..];
            let notify_cfg = config.config.notifies.get(notify_name).ok_or_else(|| {
                FrostxError::Config(format!(
                    "notify '{notify_name}' not defined in [config.notify.{notify_name}]"
                ))
            })?;
            Ok(Box::new(notify::Notify::new(notify_cfg.clone())))
        }
        name if name.starts_with("hook.") => {
            let hook_name = &name["hook.".len()..];
            let hook_cfg = config.config.hooks.get(hook_name).ok_or_else(|| {
                FrostxError::Config(format!(
                    "hook '{hook_name}' not defined in [config.hook.{hook_name}]"
                ))
            })?;
            Ok(Box::new(hook::Hook::new(hook_name, hook_cfg.clone())))
        }
        _ => Err(FrostxError::UnknownAction(
            crate::diagnostics::unknown_action_message(name),
        )),
    }
}

/// Every statically registered action name, sorted alphabetically.
///
/// Dynamic action categories (`hook.<name>`, `notify.<name>`, `group.<name>`)
/// are user-defined and not listed here.
pub const ALL_STATIC_ACTIONS: &[&str] = &[
    "git.check_clean",
    "git.check_pushed",
    "git.clean",
    "git.tag",
    "jj.check_clean",
    "jj.check_pushed",
    "jj.bookmark",
    "vcs.check_clean",
    "vcs.check_pushed",
    "vcs.mark",
    "fs.clean_artifacts",
    "archive.tar_gz",
    "backup.check",
    "backup.upload",
    "backup.verify",
    "local.delete",
];
