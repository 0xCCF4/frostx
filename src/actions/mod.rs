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

/// Factory function type: creates a [`Box<dyn Action>`] from project config.
pub type ActionFactory = fn(&ProjectConfig) -> Result<Box<dyn Action>, FrostxError>;

/// All per-module static action registries.
const ALL_REGISTRIES: &[&[(&str, ActionFactory)]] = &[
    git::REGISTRY,
    jj::REGISTRY,
    vcs::REGISTRY,
    fs::REGISTRY,
    archive::REGISTRY,
    backup::REGISTRY,
    local::REGISTRY,
];

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
    ///
    /// # Errors
    ///
    /// Returns an error if the action fails to execute (e.g., I/O error, process failure).
    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError>;
}

/// Create a boxed [`Action`] from its name string.
///
/// Looks up static actions in the per-module registries first, then handles
/// dynamic categories (`hook.<name>`, `notify.<name>`). Adding a new static
/// action only requires a new entry in the module's `REGISTRY` - no changes
/// here.
///
/// # Errors
///
/// Returns [`FrostxError::UnknownAction`] if `name` is not registered, or a
/// config error if the action requires config that is absent.
pub fn create(name: &str, config: &ProjectConfig) -> Result<Box<dyn Action>, FrostxError> {
    for registry in ALL_REGISTRIES {
        for (action_name, factory) in *registry {
            if *action_name == name {
                return factory(config);
            }
        }
    }
    if let Some(notify_name) = name.strip_prefix("notify.") {
        let notify_cfg = config.config.notifies.get(notify_name).ok_or_else(|| {
            FrostxError::Config(format!(
                "notify '{notify_name}' not defined in [config.notify.{notify_name}]"
            ))
        })?;
        return Ok(Box::new(notify::Notify::new(notify_cfg.clone())));
    }
    if let Some(hook_name) = name.strip_prefix("hook.") {
        let hook_cfg = config.config.hooks.get(hook_name).ok_or_else(|| {
            FrostxError::Config(format!(
                "hook '{hook_name}' not defined in [config.hook.{hook_name}]"
            ))
        })?;
        return Ok(Box::new(hook::Hook::new(hook_name, hook_cfg.clone())));
    }
    Err(FrostxError::UnknownAction(
        crate::diagnostics::unknown_action_message(name),
    ))
}

/// Every statically registered action name, sorted alphabetically.
///
/// Dynamic action categories (`hook.<name>`, `notify.<name>`, `group.<name>`)
/// are user-defined and not listed here. The list is derived from all module
/// registries so it stays in sync automatically.
#[must_use]
pub fn all_static_actions() -> &'static [&'static str] {
    static CACHE: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut names: Vec<&'static str> = ALL_REGISTRIES
            .iter()
            .flat_map(|r| r.iter().map(|(n, _)| *n))
            .collect();
        names.sort_unstable();
        names
    })
}
