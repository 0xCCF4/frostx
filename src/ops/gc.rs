//! `frostx gc` - remove orphaned state files.

use crate::config::state::{list_state_files, ProjectState};
use crate::error::FrostxError;
use crate::output::{GcEntry, GcOutput, FROSTX_VERSION};

use super::FrostxOpts;

/// Scan the state directory for orphaned state files and optionally delete them.
///
/// An orphaned state file is one whose recorded project path no longer exists
/// or whose UUID does not match the `frostx.toml` at that path.
/// When `dry_run` is `true`, files are reported but not deleted.
///
/// # Errors
///
/// Returns an error if the state directory cannot be read or a state file cannot be deleted.
pub fn execute(dry_run: bool, opts: &FrostxOpts) -> Result<GcOutput, FrostxError> {
    let entries = list_state_files(&opts.state_dir)?;
    let mut orphaned: Vec<GcEntry> = Vec::new();
    let mut removed = 0;

    for (uuid, state_file_path) in &entries {
        let state = ProjectState::load(&opts.state_dir, *uuid)?;
        let recorded_path = &state.project_path;

        let reason = if recorded_path.as_os_str().is_empty() {
            // State file exists but has no recorded path - orphaned.
            Some(("path_missing".to_string(), String::new()))
        } else if !recorded_path.exists() {
            Some((
                "path_missing".to_string(),
                recorded_path.display().to_string(),
            ))
        } else {
            // Path exists - check UUID matches.
            let cfg_path = recorded_path.join(crate::config::CONFIG_FILENAME);
            if cfg_path.exists() {
                // Read id from the config without full parsing.
                let content = std::fs::read_to_string(&cfg_path).unwrap_or_default();
                let recorded_uuid: Option<uuid::Uuid> = toml::from_str::<toml::Value>(&content)
                    .ok()
                    .and_then(|v| v.get("id")?.as_str().and_then(|s| s.parse().ok()));
                if recorded_uuid == Some(*uuid) {
                    None
                } else {
                    Some((
                        "uuid_mismatch".to_string(),
                        recorded_path.display().to_string(),
                    ))
                }
            } else {
                Some((
                    "path_missing".to_string(),
                    recorded_path.display().to_string(),
                ))
            }
        };

        if let Some((reason_str, path_str)) = reason {
            let filename = state_file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            orphaned.push(GcEntry {
                state_file: filename,
                reason: reason_str,
                path: path_str,
            });
            if !dry_run {
                ProjectState::delete(&opts.state_dir, *uuid)?;
                removed += 1;
            }
        }
    }

    Ok(GcOutput {
        frostx_version: FROSTX_VERSION,
        orphaned,
        removed,
    })
}
