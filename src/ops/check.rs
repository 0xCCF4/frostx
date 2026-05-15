//! `frostx check` - inspect inactivity and rule trigger status.

use crate::config::state::ProjectState;
use crate::error::FrostxError;
use crate::output::{build_check_output, CheckOutput};
use crate::pipeline;
use crate::scanner;
use std::path::Path;

use super::FrostxOpts;

/// Evaluate a project's inactivity and rule trigger status without running any actions.
///
/// Loads config and state, evaluates rules, persists the updated `project_path`,
/// and returns the rendered output ready for display or further processing.
///
/// # Errors
///
/// Returns an error if the config or state cannot be loaded, or if the scan fails.
pub fn gather(path: &Path, opts: &FrostxOpts) -> Result<CheckOutput, FrostxError> {
    let config = super::init::load_config(path, opts)?;
    super::init::check_uuid_collision(&config, path, &opts.state_dir)?;

    let mut state = ProjectState::load(&opts.state_dir, config.id)?;
    state.project_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let scan = scanner::scan(path)?;
    let last_modified = opts
        .pretend_inactive
        .as_ref()
        .map_or(scan.last_modified, |d| d.subtract_from(chrono::Utc::now()));
    let inactive_seconds = (chrono::Utc::now() - last_modified).num_seconds().max(0);
    let outcomes = pipeline::evaluate(&config, &state, last_modified)?;

    let project_name = config
        .name
        .as_deref()
        .or_else(|| state.project_path.file_name().and_then(|n| n.to_str()))
        .unwrap_or("unknown");

    let out = build_check_output(
        project_name,
        config.description.as_deref(),
        path,
        config.id,
        inactive_seconds,
        &outcomes,
    );

    state.save(&opts.state_dir, config.id)?;

    Ok(out)
}
