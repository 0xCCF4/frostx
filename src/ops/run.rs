//! `frostx run` - execute the inactivity pipeline.

use crate::config::state::ProjectState;
use crate::error::FrostxError;
use crate::pipeline::{self, ActionCallback, ActionStatus, RunOptions};
use crate::scanner;
use std::path::PathBuf;

use super::FrostxOpts;

/// Arguments for the `run` operation.
pub struct RunArgs {
    /// Project directory to run.
    pub path: PathBuf,
    /// Run only this rule number (1-indexed).
    pub rule_filter: Option<usize>,
    /// Run only this named action, bypassing threshold checks.
    pub action_filter: Option<String>,
    /// Re-execute completed mutation actions.
    pub force: bool,
}

/// Execute the inactivity pipeline for a project.
///
/// `on_action` is called after each action completes, enabling real-time
/// streaming of results. Returns `true` if any action failed.
pub fn execute(
    args: &RunArgs,
    opts: &FrostxOpts,
    on_action: &ActionCallback<'_>,
) -> Result<bool, FrostxError> {
    let path = &args.path;
    let config = super::init::load_config(path, opts)?;
    super::init::check_uuid_collision(&config, path, &opts.state_dir)?;

    let mut state = ProjectState::load(&opts.state_dir, config.id)?;
    state.project_path = path.canonicalize().unwrap_or_else(|_| path.clone());

    let scan = scanner::scan(path)?;

    let run_opts = RunOptions {
        dry_run: opts.dry_run,
        force: args.force,
        yes: opts.yes,
        rule_filter: args.rule_filter,
        action_filter: args.action_filter.clone(),
    };

    let outcomes = pipeline::run(
        &config,
        &mut state,
        path,
        scan.last_modified,
        &run_opts,
        on_action,
    )?;

    if !opts.dry_run {
        state.last_scan = Some(chrono::Utc::now());
        state.save(&opts.state_dir, config.id)?;
    }

    let had_failures = outcomes.iter().any(|r| {
        r.action_outcomes
            .iter()
            .any(|ao| ao.status == ActionStatus::Failed)
    });

    Ok(had_failures)
}
