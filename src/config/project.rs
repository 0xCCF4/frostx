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

    /// Per-tag overrides applied when the action is referenced with a `#tag` suffix.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub overrides: std::collections::HashMap<String, VcsConfigOverride>,
}

impl VcsConfig {
    /// Merge-patch the base config with the override entry for `tag`.
    ///
    /// If `tag` is `None` or no entry exists for it, the base config is returned unchanged.
    #[must_use]
    pub fn resolve(&self, tag: Option<&str>) -> ResolvedVcsConfig {
        let ov = tag.and_then(|t| self.overrides.get(t));
        ResolvedVcsConfig {
            skip_if_no_vcs: ov
                .and_then(|o| o.skip_if_no_vcs)
                .unwrap_or(self.skip_if_no_vcs),
        }
    }
}

/// Partial `[config.vcs]` values used for per-tag merge-patch overrides.
///
/// Every field is optional; absent fields fall back to the base
/// `[config.vcs]` value when [`VcsConfig::resolve`] is called.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcsConfigOverride {
    /// Override for [`VcsConfig::skip_if_no_vcs`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_if_no_vcs: Option<bool>,
}

/// Fully resolved vcs configuration after merge-patching any `#tag` override.
///
/// Produced by [`VcsConfig::resolve`]; contains no optional fields.
#[derive(Debug, Clone)]
pub struct ResolvedVcsConfig {
    /// Whether to skip silently when no VCS is detected, after override resolution.
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

    /// Per-tag overrides applied when an action is referenced with a `#tag`
    /// suffix (e.g. `backup.upload#offsite`).  Only the fields present in the
    /// override entry replace the corresponding base-config values; absent
    /// fields inherit from `[config.backup]`.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub overrides: std::collections::HashMap<String, BackupConfigOverride>,
}

/// Partial `[config.backup]` values used for per-tag merge-patch overrides.
///
/// Every field is optional; absent fields fall back to the base
/// `[config.backup]` value when [`BackupConfig::resolve`] is called.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfigOverride {
    /// Override for [`BackupConfig::server`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
}

impl BackupConfig {
    /// Merge-patch the base config with the override entry for `tag`.
    ///
    /// If `tag` is `None` or no override entry exists for it, the base
    /// config is returned unchanged.
    #[must_use]
    pub fn resolve(&self, tag: Option<&str>) -> ResolvedBackupConfig {
        let ov = tag.and_then(|t| self.overrides.get(t));
        ResolvedBackupConfig {
            server: ov
                .and_then(|o| o.server.clone())
                .unwrap_or_else(|| self.server.clone()),
        }
    }
}

/// Fully resolved backup configuration after merge-patching any `#tag` override.
///
/// Produced by [`BackupConfig::resolve`]; contains no optional fields.
#[derive(Debug, Clone)]
pub struct ResolvedBackupConfig {
    /// Backup server URL after override resolution.
    pub server: String,
}

/// `[config.archive]` section - controls how `archive.compress` compresses the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveConfig {
    #[serde(default = "ArchiveConfig::default_compression")]
    pub compression: Compression,

    /// Per-tag overrides applied when the action is referenced with a `#tag` suffix.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub overrides: std::collections::HashMap<String, ArchiveConfigOverride>,
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            compression: Compression::Gz,
            overrides: std::collections::HashMap::new(),
        }
    }
}

impl ArchiveConfig {
    fn default_compression() -> Compression {
        Compression::Gz
    }

    /// Merge-patch the base config with the override entry for `tag`.
    ///
    /// If `tag` is `None` or no entry exists for it, the base config is returned unchanged.
    #[must_use]
    pub fn resolve(&self, tag: Option<&str>) -> ResolvedArchiveConfig {
        let ov = tag.and_then(|t| self.overrides.get(t));
        ResolvedArchiveConfig {
            compression: ov
                .and_then(|o| o.compression.clone())
                .unwrap_or_else(|| self.compression.clone()),
        }
    }
}

/// Partial `[config.archive]` values used for per-tag merge-patch overrides.
///
/// Every field is optional; absent fields fall back to the base
/// `[config.archive]` value when [`ArchiveConfig::resolve`] is called.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveConfigOverride {
    /// Override for [`ArchiveConfig::compression`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<Compression>,
}

/// Fully resolved archive configuration after merge-patching any `#tag` override.
///
/// Produced by [`ArchiveConfig::resolve`]; contains no optional fields.
#[derive(Debug, Clone)]
pub struct ResolvedArchiveConfig {
    /// Compression algorithm after override resolution.
    pub compression: Compression,
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

/// `[config.fs]` section - controls `fs.clean_artifacts` behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FsConfig {
    /// Additional paths to always remove, relative to the project root.
    /// Trailing `/` is optional. Processed unconditionally regardless of marker files.
    #[serde(default)]
    pub extra_paths: Vec<String>,
    /// Per-cleaner enable flags. All cleaners are enabled by default.
    #[serde(default)]
    pub cleaners: CleanersConfig,

    /// Per-tag overrides applied when the action is referenced with a `#tag` suffix.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub overrides: std::collections::HashMap<String, FsConfigOverride>,
}

impl FsConfig {
    /// Merge-patch the base config with the override entry for `tag`.
    ///
    /// If `tag` is `None` or no entry exists for it, the base config is returned unchanged.
    #[must_use]
    pub fn resolve(&self, tag: Option<&str>) -> ResolvedFsConfig {
        let ov = tag.and_then(|t| self.overrides.get(t));
        ResolvedFsConfig {
            extra_paths: ov
                .and_then(|o| o.extra_paths.clone())
                .unwrap_or_else(|| self.extra_paths.clone()),
            cleaners: ov
                .and_then(|o| o.cleaners.clone())
                .unwrap_or_else(|| self.cleaners.clone()),
        }
    }
}

/// Partial `[config.fs]` values used for per-tag merge-patch overrides.
///
/// Every field is optional; absent fields fall back to the base
/// `[config.fs]` value when [`FsConfig::resolve`] is called.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsConfigOverride {
    /// Override for [`FsConfig::extra_paths`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_paths: Option<Vec<String>>,
    /// Override for [`FsConfig::cleaners`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleaners: Option<CleanersConfig>,
}

/// Fully resolved fs configuration after merge-patching any `#tag` override.
///
/// Produced by [`FsConfig::resolve`]; contains no optional fields.
#[derive(Debug, Clone)]
pub struct ResolvedFsConfig {
    /// Extra paths to remove after override resolution.
    pub extra_paths: Vec<String>,
    /// Cleaner flags after override resolution.
    pub cleaners: CleanersConfig,
}

/// Enables or disables each auto-detecting cleaner independently.
///
/// Each cleaner checks for a language-specific marker file before touching anything.
/// All cleaners are enabled when `[config.fs.cleaners]` is absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanersConfig {
    /// Detect `Cargo.toml`; remove `target/` if found.
    #[serde(default = "default_true")]
    pub rust: bool,
    /// Detect `package.json`; remove `node_modules/` if found.
    #[serde(default = "default_true")]
    pub node: bool,
    /// Detect `pyproject.toml` or `setup.py`; remove `.venv/` if found.
    #[serde(default = "default_true")]
    pub python: bool,
}

impl Default for CleanersConfig {
    fn default() -> Self {
        Self {
            rust: true,
            node: true,
            python: true,
        }
    }
}

fn default_true() -> bool {
    true
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
    /// When `true`, the entire rule is skipped after one successful run of all
    /// actions. Subsequent runs treat the rule as finished unless `--force` is
    /// passed. Individual action completion is still tracked separately so that
    /// a partial run can resume where it left off before the rule is sealed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub once: bool,
}

impl Rule {
    /// Compute a stable hash of this rule's identity: `after` + `actions` list.
    ///
    /// Used by state tracking to detect config changes that should reset
    /// completion records. Only `after` and `actions` contribute — changing
    /// `name` alone does not invalidate prior completions.
    #[must_use]
    pub fn rule_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.after.to_string().as_bytes());
        for action in &self.actions {
            hasher.update(b"\n");
            hasher.update(action.as_bytes());
        }
        hasher.finalize().iter().fold(String::new(), |mut acc, b| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{b:02x}");
            acc
        })
    }
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

    /// Return the raw `[config.backup]` section or an error if missing.
    ///
    /// Prefer [`Self::resolve_backup`] when constructing actions so that
    /// per-tag overrides are applied automatically.
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

    /// Resolve backup config for the given `#tag`, applying any matching
    /// override entry as a merge-patch over the base `[config.backup]` values.
    ///
    /// Pass `tag = None` to get the base config without any override.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::FrostxError::BackupConfigMissing`] if `[config.backup]` is absent.
    pub fn resolve_backup(
        &self,
        tag: Option<&str>,
    ) -> Result<ResolvedBackupConfig, crate::error::FrostxError> {
        Ok(self.require_backup()?.resolve(tag))
    }

    /// Resolve archive config for the given `#tag`, applying any matching
    /// override entry as a merge-patch over the base `[config.archive]` values.
    ///
    /// Falls back to [`ArchiveConfig::default`] when `[config.archive]` is absent.
    #[must_use]
    pub fn resolve_archive(&self, tag: Option<&str>) -> ResolvedArchiveConfig {
        self.config
            .archive
            .as_ref()
            .map_or_else(|| ArchiveConfig::default().resolve(tag), |a| a.resolve(tag))
    }

    /// Resolve fs config for the given `#tag`, applying any matching override
    /// entry as a merge-patch over the base `[config.fs]` values.
    ///
    /// Falls back to [`FsConfig::default`] when `[config.fs]` is absent.
    #[must_use]
    pub fn resolve_fs(&self, tag: Option<&str>) -> ResolvedFsConfig {
        self.config
            .fs
            .as_ref()
            .map_or_else(|| FsConfig::default().resolve(tag), |f| f.resolve(tag))
    }

    /// Resolve vcs config for the given `#tag`, applying any matching override
    /// entry as a merge-patch over the base `[config.vcs]` values.
    ///
    /// Falls back to [`VcsConfig::default`] when `[config.vcs]` is absent.
    #[must_use]
    pub fn resolve_vcs(&self, tag: Option<&str>) -> ResolvedVcsConfig {
        self.config
            .vcs
            .as_ref()
            .map_or_else(|| VcsConfig::default().resolve(tag), |v| v.resolve(tag))
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
            once: false,
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
            once: false,
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
            once: false,
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
            once: false,
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
            once: false,
        });
        assert!(cfg.expand_groups().is_err());
    }

    #[test]
    fn compression_extension() {
        assert_eq!(Compression::Gz.extension(), "tar.gz");
        assert_eq!(Compression::Zstd.extension(), "tar.zst");
        assert_eq!(Compression::Xz.extension(), "tar.xz");
    }

    #[test]
    fn backup_resolve_no_tag_returns_base() {
        let cfg = BackupConfig {
            server: "rsync://base.example.com/".into(),
            overrides: std::collections::HashMap::new(),
        };
        assert_eq!(cfg.resolve(None).server, "rsync://base.example.com/");
    }

    #[test]
    fn backup_resolve_unknown_tag_falls_back_to_base() {
        let cfg = BackupConfig {
            server: "rsync://base.example.com/".into(),
            overrides: std::collections::HashMap::new(),
        };
        assert_eq!(
            cfg.resolve(Some("nonexistent")).server,
            "rsync://base.example.com/"
        );
    }

    #[test]
    fn backup_resolve_tag_overrides_server() {
        let mut cfg = BackupConfig {
            server: "rsync://base.example.com/".into(),
            overrides: std::collections::HashMap::new(),
        };
        cfg.overrides.insert(
            "offsite".into(),
            BackupConfigOverride {
                server: Some("rsync://offsite.example.com/".into()),
            },
        );
        assert_eq!(
            cfg.resolve(Some("offsite")).server,
            "rsync://offsite.example.com/"
        );
        assert_eq!(cfg.resolve(None).server, "rsync://base.example.com/");
    }

    #[test]
    fn backup_resolve_tag_with_absent_server_inherits_base() {
        let mut cfg = BackupConfig {
            server: "rsync://base.example.com/".into(),
            overrides: std::collections::HashMap::new(),
        };
        cfg.overrides
            .insert("partial".into(), BackupConfigOverride { server: None });
        assert_eq!(
            cfg.resolve(Some("partial")).server,
            "rsync://base.example.com/"
        );
    }
}
