//! `frostx scan` - walk a directory tree and report all managed projects.

use crate::config;
use crate::config::state::ProjectState;
use crate::error::FrostxError;
use crate::output::{build_check_output, CheckOutput};
use crate::pipeline;
use crate::scanner;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::FrostxOpts;

/// Arguments for the `scan` operation.
pub struct ScanArgs {
    /// Root directory to walk.
    pub root: PathBuf,
    /// Only report projects with at least one triggered rule.
    pub triggered_only: bool,
    /// Maximum walk depth; `None` means unlimited.
    pub depth: Option<usize>,
}

/// Walk a directory tree and evaluate all managed projects.
///
/// Projects that fail to load or scan are silently skipped.
/// Returns one [`CheckOutput`] per successfully evaluated project.
///
/// # Errors
///
/// This function currently always returns `Ok`; the signature reserves the right
/// to propagate fatal I/O errors in future versions.
pub fn execute(args: &ScanArgs, opts: &FrostxOpts) -> Result<Vec<CheckOutput>, FrostxError> {
    let projects = find_projects(&args.root, args.depth);
    let mut results = Vec::new();

    for project_dir in projects {
        let Ok(cfg) = config::load(&project_dir, &opts.library_dir) else {
            continue;
        };

        let mut state = ProjectState::load(&opts.state_dir, cfg.id).unwrap_or_default();
        state.project_path = project_dir.canonicalize().unwrap_or(project_dir.clone());

        let Ok(scan) = scanner::scan(&project_dir) else {
            continue;
        };

        let last_modified = opts
            .pretend_inactive
            .as_ref()
            .map_or(scan.last_modified, |d| d.subtract_from(chrono::Utc::now()));
        let inactive_seconds = (chrono::Utc::now() - last_modified).num_seconds().max(0);

        let Ok(outcomes) = pipeline::evaluate(&cfg, &state, last_modified) else {
            continue;
        };

        if args.triggered_only && !outcomes.iter().any(|r| r.triggered) {
            continue;
        }

        let project_name = cfg
            .name
            .as_deref()
            .or_else(|| project_dir.file_name().and_then(|n| n.to_str()))
            .unwrap_or("unknown");

        results.push(build_check_output(
            project_name,
            cfg.description.as_deref(),
            &project_dir,
            cfg.id,
            inactive_seconds,
            &outcomes,
        ));
    }

    Ok(results)
}

/// Walk `root` and return all directories containing a `frostx.toml`.
pub fn find_projects(root: &Path, max_depth: Option<usize>) -> Vec<PathBuf> {
    let mut walker = WalkDir::new(root).follow_links(false);
    if let Some(d) = max_depth {
        walker = walker.max_depth(d);
    }
    walker
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_name() == config::CONFIG_FILENAME && e.file_type().is_file())
        .filter_map(|e| e.path().parent().map(Path::to_path_buf))
        .collect()
}
