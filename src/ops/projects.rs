//! `frostx projects` - manage the tracked-project registry.

use crate::config;
use crate::config::state::{list_state_files, ProjectState};
use crate::error::FrostxError;
use crate::output::{
    CheckOutput, ProjectAddOutput, ProjectAddSkip, ProjectEntry, ProjectRmOutput,
    ProjectsListOutput, FROSTX_VERSION,
};
use crate::pipeline;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::FrostxOpts;

/// Arguments for `projects add`.
pub struct ProjectsAddArgs {
    /// Explicit project directories to register.
    pub paths: Vec<PathBuf>,
    /// When set, recursively scan this directory and register every project found.
    pub scan_dir: Option<PathBuf>,
}

/// Arguments for `projects run`.
pub struct ProjectsRunArgs {
    /// Re-execute completed mutation actions.
    pub force: bool,
    /// Run only rule number N (1-indexed).
    pub rule_filter: Option<usize>,
    /// Run only this named action, bypassing threshold checks.
    pub action_filter: Option<String>,
}

/// List all currently tracked projects.
///
/// # Errors
///
/// Returns an error if the state directory cannot be read.
pub fn list(opts: &FrostxOpts) -> Result<ProjectsListOutput, FrostxError> {
    let entries = list_state_files(&opts.state_dir)?;
    let mut projects = Vec::new();
    for (uuid, _) in entries {
        let state = ProjectState::load(&opts.state_dir, uuid)?;
        let (name, description) = config::load(&state.project_path, &opts.library_dir)
            .ok()
            .map_or((None, None), |cfg| (cfg.name, cfg.description));
        projects.push(ProjectEntry {
            uuid: uuid.to_string(),
            path: state.project_path.display().to_string(),
            name,
            description,
            last_scan: state.last_scan.map(|t| t.to_rfc3339()),
        });
    }
    Ok(ProjectsListOutput {
        frostx_version: FROSTX_VERSION,
        projects,
    })
}

/// Register one or more projects, optionally discovering them via `scan_dir`.
///
/// Projects that fail are included in `skipped` rather than propagating as
/// errors, so callers always receive a complete picture of what happened.
#[must_use]
pub fn add(args: &ProjectsAddArgs, opts: &FrostxOpts) -> ProjectAddOutput {
    let mut all_paths: Vec<PathBuf> = args.paths.clone();
    if let Some(ref dir) = args.scan_dir {
        all_paths.extend(super::scan::find_projects(dir, None));
    }

    let mut added = Vec::new();
    let mut skipped = Vec::new();

    for path in &all_paths {
        match add_single(path, opts) {
            Ok(entry) => added.push(entry),
            Err(e) => skipped.push(ProjectAddSkip {
                path: path.display().to_string(),
                reason: e.to_string(),
            }),
        }
    }

    ProjectAddOutput {
        frostx_version: FROSTX_VERSION,
        added,
        skipped,
    }
}

fn add_single(path: &Path, opts: &FrostxOpts) -> Result<ProjectEntry, FrostxError> {
    let cfg = config::load(path, &opts.library_dir)?;
    let canonical = path.canonicalize()?;

    let mut state = ProjectState::load(&opts.state_dir, cfg.id)?;

    if !state.project_path.as_os_str().is_empty() && state.project_path != canonical {
        return Err(FrostxError::UuidCollision {
            current: canonical,
            recorded: state.project_path.clone(),
        });
    }

    let last_scan = state.last_scan.map(|t| t.to_rfc3339());
    state.project_path.clone_from(&canonical);
    state.save(&opts.state_dir, cfg.id)?;

    Ok(ProjectEntry {
        uuid: cfg.id.to_string(),
        path: canonical.display().to_string(),
        name: cfg.name.clone(),
        description: cfg.description.clone(),
        last_scan,
    })
}

/// Unregister a project by deleting its state file.
///
/// # Errors
///
/// Returns an error if the project path cannot be resolved, the UUID cannot be
/// found, or the state file cannot be deleted.
pub fn rm(path: &Path, opts: &FrostxOpts) -> Result<ProjectRmOutput, FrostxError> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let uuid = if let Ok(cfg) = config::load(path, &opts.library_dir) {
        cfg.id
    } else {
        find_uuid_by_path(&canonical, opts)?
    };

    ProjectState::delete(&opts.state_dir, uuid)?;

    Ok(ProjectRmOutput {
        frostx_version: FROSTX_VERSION,
        uuid: uuid.to_string(),
        path: canonical.display().to_string(),
    })
}

fn find_uuid_by_path(canonical: &Path, opts: &FrostxOpts) -> Result<Uuid, FrostxError> {
    let entries = list_state_files(&opts.state_dir)?;
    for (uuid, _) in entries {
        let state = ProjectState::load(&opts.state_dir, uuid)?;
        if state.project_path == canonical {
            return Ok(uuid);
        }
    }
    Err(FrostxError::NotInitialized(canonical.to_path_buf()))
}

/// Run `check` on every tracked project.
///
/// Returns `(successes, errors)`. Errors are per-project and do not abort the
/// rest of the scan.
#[must_use]
pub fn check_all(opts: &FrostxOpts) -> (Vec<CheckOutput>, Vec<(PathBuf, FrostxError)>) {
    let entries = match list_state_files(&opts.state_dir) {
        Ok(e) => e,
        Err(e) => return (vec![], vec![(PathBuf::new(), e)]),
    };

    let mut results = Vec::new();
    let mut errors = Vec::new();

    for (uuid, _) in entries {
        let Some(path) = tracked_path(&opts.state_dir, uuid) else {
            continue;
        };

        match super::check::gather(&path, opts) {
            Ok(out) => results.push(out),
            Err(e) => errors.push((path, e)),
        }
    }

    (results, errors)
}

/// Execute the inactivity pipeline for every tracked project.
///
/// `on_action` is called after each action with the project path, rule index,
/// optional rule name, and action outcome, enabling real-time output streaming.
///
/// Returns `(had_failures, errors)` where `had_failures` is `true` if any
/// action status was `Failed`, and `errors` lists projects that could not be
/// loaded or run.
#[allow(clippy::type_complexity)]
pub fn run_all(
    args: &ProjectsRunArgs,
    opts: &FrostxOpts,
    on_action: &dyn Fn(&Path, usize, Option<&str>, &pipeline::ActionOutcome),
) -> (bool, Vec<(PathBuf, FrostxError)>) {
    let entries = match list_state_files(&opts.state_dir) {
        Ok(e) => e,
        Err(e) => return (false, vec![(PathBuf::new(), e)]),
    };

    let mut had_failures = false;
    let mut errors = Vec::new();

    for (uuid, _) in entries {
        let Some(path) = tracked_path(&opts.state_dir, uuid) else {
            continue;
        };

        let run_args = super::run::RunArgs {
            path: path.clone(),
            rule_filter: args.rule_filter,
            action_filter: args.action_filter.clone(),
            force: args.force,
        };

        let path_ref = path.clone();
        let cb: pipeline::ActionCallback<'_> =
            Box::new(move |rule_idx, rule_name, ao| on_action(&path_ref, rule_idx, rule_name, ao));

        match super::run::execute(&run_args, opts, &cb) {
            Ok(failed) => {
                if failed {
                    had_failures = true;
                }
            }
            Err(e) => errors.push((path, e)),
        }
    }

    (had_failures, errors)
}

fn tracked_path(state_dir: &Path, uuid: Uuid) -> Option<PathBuf> {
    let state = ProjectState::load(state_dir, uuid).ok()?;
    if state.project_path.as_os_str().is_empty() || !state.project_path.exists() {
        return None;
    }
    Some(state.project_path)
}
