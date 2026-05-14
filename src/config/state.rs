use crate::error::FrostxError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Mutable runtime state stored at `$XDG_DATA_HOME/frostx/<uuid>.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectState {
    /// Last known absolute path of the project directory.
    pub project_path: PathBuf,

    /// Timestamp of the last `frostx run` or `frostx check`.
    pub last_scan: Option<DateTime<Utc>>,

    /// Per-rule completion records.
    #[serde(default, rename = "rule")]
    pub rules: Vec<RuleState>,
}

/// State for one `[[rule]]` entry (1-indexed to match the config).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleState {
    pub index: usize,
    /// Names of mutation actions that have been successfully completed.
    #[serde(default)]
    pub completed: Vec<String>,
    pub last_run: Option<DateTime<Utc>>,
}

impl ProjectState {
    /// Load state for `uuid` from the state directory, or return a fresh default.
    ///
    /// # Errors
    ///
    /// Returns an error if the state file exists but cannot be read or parsed.
    pub fn load(state_dir: &Path, uuid: Uuid) -> Result<Self, FrostxError> {
        let path = state_file_path(state_dir, uuid);
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content)
            .map_err(|e| FrostxError::Config(format!("state file parse error: {e}")))
    }

    /// Persist state to `<state_dir>/<uuid>.toml`.
    ///
    /// # Errors
    ///
    /// Returns an error if the state directory cannot be created or the file cannot be written.
    pub fn save(&self, state_dir: &Path, uuid: Uuid) -> Result<(), FrostxError> {
        std::fs::create_dir_all(state_dir)?;
        let path = state_file_path(state_dir, uuid);
        let content = toml::to_string_pretty(self)
            .map_err(|e| FrostxError::Config(format!("state serialisation error: {e}")))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Remove the state file for `uuid` (used by `gc`).
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be removed.
    pub fn delete(state_dir: &Path, uuid: Uuid) -> Result<(), FrostxError> {
        let path = state_file_path(state_dir, uuid);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Return the state for rule `index` (1-indexed), creating it if absent.
    #[must_use]
    pub fn rule_mut(&mut self, index: usize) -> &mut RuleState {
        if let Some(pos) = self.rules.iter().position(|r| r.index == index) {
            &mut self.rules[pos]
        } else {
            let pos = self.rules.len();
            self.rules.push(RuleState {
                index,
                completed: vec![],
                last_run: None,
            });
            &mut self.rules[pos]
        }
    }

    /// Return the state for rule `index`, or `None` if absent.
    #[must_use]
    pub fn rule(&self, index: usize) -> Option<&RuleState> {
        self.rules.iter().find(|r| r.index == index)
    }

    /// Check if mutation action `name` in rule `index` is already completed.
    #[must_use]
    pub fn is_completed(&self, rule_index: usize, action_name: &str) -> bool {
        self.rule(rule_index)
            .is_some_and(|r| r.completed.iter().any(|a| a == action_name))
    }

    /// Mark mutation action `name` in rule `index` as completed.
    pub fn mark_completed(&mut self, rule_index: usize, action_name: &str) {
        let rule = self.rule_mut(rule_index);
        if !rule.completed.iter().any(|a| a == action_name) {
            rule.completed.push(action_name.to_string());
        }
        rule.last_run = Some(Utc::now());
    }
}

/// Returns all (uuid, path) pairs found in the state directory.
///
/// # Errors
///
/// Returns an error if the directory cannot be read.
pub fn list_state_files(state_dir: &Path) -> Result<Vec<(Uuid, PathBuf)>, FrostxError> {
    if !state_dir.exists() {
        return Ok(vec![]);
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(state_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(uuid) = stem.parse::<Uuid>() {
                    entries.push((uuid, path));
                }
            }
        }
    }
    Ok(entries)
}

fn state_file_path(state_dir: &Path, uuid: Uuid) -> PathBuf {
    state_dir.join(format!("{uuid}.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = tempdir().unwrap();
        let uuid = Uuid::new_v4();
        let mut state = ProjectState {
            project_path: PathBuf::from("/some/project"),
            last_scan: Some(Utc::now()),
            ..Default::default()
        };
        state.mark_completed(1, "archive.tar_gz");

        state.save(tmp.path(), uuid).unwrap();
        let loaded = ProjectState::load(tmp.path(), uuid).unwrap();

        assert_eq!(loaded.project_path, PathBuf::from("/some/project"));
        assert!(loaded.is_completed(1, "archive.tar_gz"));
        assert!(!loaded.is_completed(1, "backup.upload"));
    }

    #[test]
    fn missing_state_returns_default() {
        let tmp = tempdir().unwrap();
        let state = ProjectState::load(tmp.path(), Uuid::new_v4()).unwrap();
        assert!(state.project_path.as_os_str().is_empty());
    }

    #[test]
    fn mark_completed_idempotent() {
        let mut state = ProjectState::default();
        state.mark_completed(1, "archive.tar_gz");
        state.mark_completed(1, "archive.tar_gz");
        assert_eq!(state.rule(1).unwrap().completed.len(), 1);
    }

    #[test]
    fn list_state_files_finds_entries() {
        let tmp = tempdir().unwrap();
        let uuid = Uuid::new_v4();
        let state = ProjectState {
            project_path: PathBuf::from("/test"),
            ..Default::default()
        };
        state.save(tmp.path(), uuid).unwrap();

        let files = list_state_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, uuid);
    }

    #[test]
    fn delete_removes_file() {
        let tmp = tempdir().unwrap();
        let uuid = Uuid::new_v4();
        let state = ProjectState {
            project_path: PathBuf::from("/test"),
            ..Default::default()
        };
        state.save(tmp.path(), uuid).unwrap();

        ProjectState::delete(tmp.path(), uuid).unwrap();
        assert!(list_state_files(tmp.path()).unwrap().is_empty());
    }
}
