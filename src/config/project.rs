use super::duration::Duration;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Parsed and resolved `frostx.toml` configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Stable project identifier, assigned on `frostx init`.
    pub id: Uuid,

    /// Optional human-readable project name shown in output and logs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional description of this project shown in output and logs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Include sources - resolved before this struct is returned to callers.
    #[serde(default)]
    pub include: Vec<String>,

    /// Template variable values for `{{key}}` substitution in included files.
    ///
    /// Keys must be alphanumeric or underscores. Include files may reference
    /// these via `{{key}}` placeholders, which are replaced before TOML parsing.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub template: HashMap<String, String>,

    /// Named action groups, keyed by group name (without the `group.` prefix).
    #[serde(default, rename = "group")]
    pub groups: HashMap<String, Group>,

    /// Per-category action configuration.
    #[serde(default)]
    pub config: ActionConfig,

    /// Inactivity rules in declaration order.
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

/// A named list of reusable action references.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub actions: Vec<String>,
}

/// Top-level `[config]` section.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionConfig {
    pub backup: Option<BackupConfig>,
    pub archive: Option<ArchiveConfig>,
    pub fs: Option<FsConfig>,
    pub vcs: Option<VcsConfig>,
    #[serde(default, rename = "hook")]
    pub hooks: HashMap<String, HookConfig>,
    #[serde(default, rename = "notify")]
    pub notifies: HashMap<String, NotifyConfig>,
}

/// `[config.vcs]` section - controls VCS-agnostic action behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VcsConfig {
    /// When `true`, `vcs.*` actions skip silently if no supported VCS is detected.
    /// Default: `false` - fail when no VCS repository is found.
    #[serde(default)]
    pub skip_if_no_vcs: bool,
}

/// `[config.notify.<name>]` section - configures a named user notification that
/// pauses the pipeline until the user explicitly confirms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyConfig {
    /// Message displayed to the user before the confirmation prompt.
    pub message: String,
}

/// `[config.backup]` section - required when using any `backup.*` action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    /// Backup server URL. Supported schemes: `rsync://`, `ssh://`.
    pub server: String,
}

/// `[config.archive]` section - controls how `archive.compress` compresses the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveConfig {
    #[serde(default = "ArchiveConfig::default_compression")]
    pub compression: Compression,
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            compression: Compression::Gz,
        }
    }
}

impl ArchiveConfig {
    fn default_compression() -> Compression {
        Compression::Gz
    }
}

/// Compression algorithm for `archive.compress`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    /// gzip (`.tar.gz`).
    Gz,
    /// Zstandard (`.tar.zst`).
    Zstd,
    /// XZ (`.tar.xz`).
    Xz,
}

impl Compression {
    /// Returns the file extension produced by this algorithm (e.g. `"tar.gz"`).
    #[must_use]
    pub fn extension(&self) -> &str {
        match self {
            Self::Gz => "tar.gz",
            Self::Zstd => "tar.zst",
            Self::Xz => "tar.xz",
        }
    }
}

/// `[config.fs]` section - controls which artifact directories `fs.clean_artifacts` removes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsConfig {
    /// Directory names relative to the project root that will be removed.
    #[serde(default = "FsConfig::default_clean_artifacts")]
    pub clean_artifacts: Vec<String>,
}

impl Default for FsConfig {
    fn default() -> Self {
        Self {
            clean_artifacts: Self::default_clean_artifacts(),
        }
    }
}

impl FsConfig {
    /// The built-in artifact directory list used when `[config.fs]` is absent.
    #[must_use]
    pub fn default_clean_artifacts() -> Vec<String> {
        vec![
            "target/".into(),
            "node_modules/".into(),
            ".venv/".into(),
            "dist/".into(),
            "build/".into(),
            ".cache/".into(),
        ]
    }
}

/// `[config.hook.<name>]` section - defines a named shell command action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Shell command executed via `sh -c` in the project directory.
    pub command: String,
    /// Whether the hook behaves as a check or a mutation.
    #[serde(default)]
    pub kind: HookKind,
    /// Allow this hook to run when the project has been compressed to an
    /// archive file. When `false` (default), the hook is skipped (check) or
    /// fails (mutation) if `project_path` is a file rather than a directory.
    #[serde(default)]
    pub run_on_archive: bool,
}

/// Whether a hook action is a check (re-runs every time) or a mutation (recorded as completed).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HookKind {
    /// Recorded as completed after success; skipped on re-runs unless `--force`.
    #[default]
    Mutation,
    /// Re-evaluated on every run; never recorded as completed.
    Check,
}

/// One `[[rule]]` block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Optional human-readable label shown in output and logs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub after: Duration,
    pub actions: Vec<String>,
}

impl ProjectConfig {
    /// Expand all `group.<name>` references in every rule's action list.
    ///
    /// # Errors
    ///
    /// Returns an error if a referenced group is not defined or if a circular
    /// group reference is detected.
    pub fn expand_groups(&self) -> Result<Vec<Vec<String>>, crate::error::FrostxError> {
        self.rules
            .iter()
            .map(|rule| expand_action_list(&rule.actions, &self.groups, &mut vec![]))
            .collect()
    }

    /// Return the `[config.backup]` section or an error if missing.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::FrostxError::BackupConfigMissing`] if `[config.backup]` is absent.
    pub fn require_backup(&self) -> Result<&BackupConfig, crate::error::FrostxError> {
        self.config
            .backup
            .as_ref()
            .ok_or(crate::error::FrostxError::BackupConfigMissing)
    }
}

fn expand_action_list(
    actions: &[String],
    groups: &HashMap<String, Group>,
    visited: &mut Vec<String>,
) -> Result<Vec<String>, crate::error::FrostxError> {
    let mut out = Vec::new();
    for action in actions {
        if let Some(group_name) = action.strip_prefix("group.") {
            if visited.contains(&group_name.to_string()) {
                return Err(crate::error::FrostxError::Config(format!(
                    "circular group reference: {group_name}"
                )));
            }
            let group = groups.get(group_name).ok_or_else(|| {
                crate::error::FrostxError::Config(format!("unknown group: {group_name}"))
            })?;
            visited.push(group_name.to_string());
            let expanded = expand_action_list(&group.actions, groups, visited)?;
            visited.pop();
            out.extend(expanded);
        } else {
            out.push(action.clone());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_config(id: Uuid) -> ProjectConfig {
        ProjectConfig {
            id,
            name: None,
            description: None,
            include: vec![],
            template: HashMap::new(),
            groups: HashMap::new(),
            config: ActionConfig::default(),
            rules: vec![],
        }
    }

    #[test]
    fn expand_no_groups() {
        let mut cfg = minimal_config(Uuid::new_v4());
        cfg.rules.push(Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["git.check_clean".into(), "git.check_pushed".into()],
        });
        let expanded = cfg.expand_groups().unwrap();
        assert_eq!(expanded[0], vec!["git.check_clean", "git.check_pushed"]);
    }

    #[test]
    fn expand_simple_group() {
        let mut cfg = minimal_config(Uuid::new_v4());
        cfg.groups.insert(
            "checks".into(),
            Group {
                actions: vec!["git.check_clean".into(), "git.check_pushed".into()],
            },
        );
        cfg.rules.push(Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["group.checks".into(), "backup.check".into()],
        });
        let expanded = cfg.expand_groups().unwrap();
        assert_eq!(
            expanded[0],
            vec!["git.check_clean", "git.check_pushed", "backup.check"]
        );
    }

    #[test]
    fn expand_nested_group() {
        let mut cfg = minimal_config(Uuid::new_v4());
        cfg.groups.insert(
            "git".into(),
            Group {
                actions: vec!["git.check_clean".into()],
            },
        );
        cfg.groups.insert(
            "all".into(),
            Group {
                actions: vec!["group.git".into(), "backup.check".into()],
            },
        );
        cfg.rules.push(Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["group.all".into()],
        });
        let expanded = cfg.expand_groups().unwrap();
        assert_eq!(expanded[0], vec!["git.check_clean", "backup.check"]);
    }

    #[test]
    fn circular_group_detected() {
        let mut cfg = minimal_config(Uuid::new_v4());
        cfg.groups.insert(
            "a".into(),
            Group {
                actions: vec!["group.b".into()],
            },
        );
        cfg.groups.insert(
            "b".into(),
            Group {
                actions: vec!["group.a".into()],
            },
        );
        cfg.rules.push(Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["group.a".into()],
        });
        assert!(cfg.expand_groups().is_err());
    }

    #[test]
    fn unknown_group_error() {
        let mut cfg = minimal_config(Uuid::new_v4());
        cfg.rules.push(Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["group.missing".into()],
        });
        assert!(cfg.expand_groups().is_err());
    }

    #[test]
    fn compression_extension() {
        assert_eq!(Compression::Gz.extension(), "tar.gz");
        assert_eq!(Compression::Zstd.extension(), "tar.zst");
        assert_eq!(Compression::Xz.extension(), "tar.xz");
    }
}
