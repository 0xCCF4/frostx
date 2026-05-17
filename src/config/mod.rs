//! Configuration loading, validation, and state management.

/// Streaming reader for `frostx.toml` and TOML fragments inside compressed archives.
pub mod archive_reader;
/// Once-per-day automation state stored in `$XDG_DATA_HOME/frostx/daily.toml`.
pub mod daily;
/// Duration parsing and elapsed-time arithmetic.
pub mod duration;
/// `include` directive resolution - merges library/relative/absolute configs.
pub mod include;
/// `frostx.toml` schema types and group-expansion logic.
pub mod project;
/// Per-project runtime state stored in `$XDG_DATA_HOME/frostx/<uuid>.toml`.
pub mod state;

use crate::error::FrostxError;
use crate::{actions, diagnostics};
use project::ProjectConfig;
use std::path::{Path, PathBuf};

/// The standard configuration filename placed in every managed project directory.
pub const CONFIG_FILENAME: &str = "frostx.toml";

/// Locate and parse `frostx.toml` in `dir`, then resolve all includes.
///
/// If `dir` is a file rather than a directory, it is treated as a compressed
/// tar archive (created by `archive.compress`) and `frostx.toml` is read
/// directly from within the archive without extracting it to disk.
///
/// # Errors
///
/// Returns an error if the config file is missing, cannot be read, fails TOML
/// parsing, or any included file cannot be resolved.
pub fn load(dir: &Path, library_dir: &Path) -> Result<ProjectConfig, FrostxError> {
    if dir.is_file() {
        return load_from_archive(dir, library_dir);
    }
    let path = config_path(dir);
    if !path.exists() {
        return Err(FrostxError::NotInitialized(dir.to_path_buf()));
    }
    let content = std::fs::read_to_string(&path)?;
    let cfg: ProjectConfig = toml::from_str(&content)
        .map_err(|e| FrostxError::Config(diagnostics::format_toml_error(&e, &path)))?;
    include::resolve_includes(cfg, dir, library_dir)
}

/// Load a project config from a compressed tar archive by streaming its
/// contents without extraction.
///
/// All `.toml` files inside the archive are buffered into memory in a single
/// pass; `frostx.toml` is parsed as the primary config and relative includes
/// are resolved against the in-memory buffer.
///
/// # Errors
///
/// Returns an error if the archive cannot be read, does not contain
/// `frostx.toml`, or any referenced include cannot be resolved.
fn load_from_archive(
    archive_path: &Path,
    library_dir: &Path,
) -> Result<ProjectConfig, FrostxError> {
    let entries = archive_reader::read_toml_entries(archive_path)?;

    let config_content = entries
        .get("frostx.toml")
        .ok_or_else(|| FrostxError::NotInitialized(archive_path.to_path_buf()))?;

    let cfg: ProjectConfig = toml::from_str(config_content).map_err(|e| {
        FrostxError::Config(format!(
            "config error in {} (inside archive {}): {e}",
            "frostx.toml",
            archive_path.display()
        ))
    })?;

    include::resolve_includes_from_archive(cfg, library_dir, &entries)
}

/// Write a freshly initialized `frostx.toml` to `dir`.
///
/// # Errors
///
/// Returns an error if the config cannot be serialized or the file cannot be written.
pub fn write_initial(dir: &Path, cfg: &ProjectConfig) -> Result<(), FrostxError> {
    let content = toml::to_string_pretty(cfg)
        .map_err(|e| FrostxError::Config(format!("serialisation error: {e}")))?;
    std::fs::write(config_path(dir), content)?;
    Ok(())
}

/// Return the expected path to `frostx.toml` inside `dir`.
#[must_use]
pub fn config_path(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILENAME)
}

/// Validate a config for structural correctness (used by `doctor`).
pub struct ValidationResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Check that a `#tag` suffix on `base_action` resolves to a defined override
/// entry for the action's category.  Returns an error message if invalid, or
/// `None` when the tag is valid (or the category has no override support).
fn validate_action_tag(
    cfg: &ProjectConfig,
    loc: &str,
    base_action: &str,
    tag: &str,
) -> Option<String> {
    // (category name, Some(true) = tag defined, Some(false) = section present but tag absent, None = section absent)
    let entries: &[(&str, Option<bool>)] = &[
        (
            "backup",
            cfg.config
                .backup
                .as_ref()
                .map(|c| c.overrides.contains_key(tag)),
        ),
        (
            "archive",
            cfg.config
                .archive
                .as_ref()
                .map(|c| c.overrides.contains_key(tag)),
        ),
        (
            "fs",
            cfg.config
                .fs
                .as_ref()
                .map(|c| c.overrides.contains_key(tag)),
        ),
        (
            "vcs",
            cfg.config
                .vcs
                .as_ref()
                .map(|c| c.overrides.contains_key(tag)),
        ),
    ];

    let &(cat, found) = entries
        .iter()
        .find(|(cat, _)| base_action.starts_with(&format!("{cat}.")))?;

    // [config.backup] absence is validated separately; suppress the tag error here.
    if cat == "backup" && found.is_none() {
        return None;
    }

    (found != Some(true)).then(|| {
        format!(
            "{loc}: {cat} tag '{tag}' not defined in \
             [config.{cat}.overrides.{tag}]"
        )
    })
}

/// Validate `cfg` for structural correctness without running anything.
///
/// Returns lists of errors and warnings. Callers should treat a non-empty
/// `errors` list as fatal and a non-empty `warnings` list as advisory.
#[must_use]
pub fn validate(cfg: &ProjectConfig) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if cfg.id.is_nil() {
        errors.push("field 'id' is missing or nil".into());
    }
    for (i, rule) in cfg.rules.iter().enumerate() {
        let idx = i + 1;
        let rule_label = match rule.name.as_deref() {
            Some(n) => format!("rule[{idx}: {n}]"),
            None => format!("rule[{idx}]"),
        };
        if rule.actions.is_empty() {
            warnings.push(format!("{rule_label}: actions list is empty"));
        }
        for (j, action) in rule.actions.iter().enumerate() {
            let loc = format!("{rule_label}.actions[{}]", j + 1);
            // Split off any `#tag` suffix for base-name lookup, keeping the
            // tag to validate that a matching override entry exists.
            let (base_action, tag) = action
                .split_once('#')
                .map_or((action.as_str(), None), |(b, t)| (b, Some(t)));
            if let Some(group_name) = base_action.strip_prefix("group.") {
                if !cfg.groups.contains_key(group_name) {
                    errors.push(format!("{loc}: unknown group '{group_name}'"));
                }
            } else if let Some(hook_name) = base_action.strip_prefix("hook.") {
                if !cfg.config.hooks.contains_key(hook_name) {
                    errors.push(format!(
                        "{loc}: hook '{hook_name}' not defined in [config.hook.{hook_name}]"
                    ));
                }
            } else if let Some(notify_name) = base_action.strip_prefix("notify.") {
                if !cfg.config.notifies.contains_key(notify_name) {
                    errors.push(format!(
                        "{loc}: notify '{notify_name}' not defined in [config.notify.{notify_name}]"
                    ));
                }
            } else if !actions::all_static_actions().contains(&base_action) {
                errors.push(format!(
                    "{loc}: {}",
                    diagnostics::unknown_action_hint(action)
                ));
            } else if let Some(t) = tag {
                if let Some(err) = validate_action_tag(cfg, &loc, base_action, t) {
                    errors.push(err);
                }
            }
        }
    }
    // Check backup config is present if backup actions are used.
    let all_actions: Vec<&str> = cfg
        .rules
        .iter()
        .flat_map(|r| r.actions.iter().map(String::as_str))
        .collect();
    let needs_backup = all_actions.iter().any(|a| a.starts_with("backup."));
    if needs_backup && cfg.config.backup.is_none() {
        errors.push("backup actions used but [config.backup] is missing".into());
    }

    // Warn on group expansion errors.
    if let Err(e) = cfg.expand_groups() {
        errors.push(format!("group expansion error: {e}"));
    }

    ValidationResult { errors, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::duration::Duration;
    use crate::config::project::{ActionConfig, Rule};
    use std::collections::HashMap;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn minimal_cfg() -> ProjectConfig {
        ProjectConfig {
            id: Uuid::new_v4(),
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
    fn write_and_load_roundtrip() {
        let tmp = tempdir().unwrap();
        let cfg = minimal_cfg();
        write_initial(tmp.path(), &cfg).unwrap();
        let loaded = load(tmp.path(), &PathBuf::from("/nonexistent")).unwrap();
        assert_eq!(loaded.id, cfg.id);
    }

    #[test]
    fn load_missing_returns_not_initialized() {
        let tmp = tempdir().unwrap();
        let err = load(tmp.path(), &PathBuf::from("/nonexistent")).unwrap_err();
        assert!(matches!(err, FrostxError::NotInitialized(_)));
    }

    #[test]
    fn validate_clean_config() {
        let cfg = minimal_cfg();
        let result = validate(&cfg);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn validate_nil_uuid_is_error() {
        let mut cfg = minimal_cfg();
        cfg.id = Uuid::nil();
        let result = validate(&cfg);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn validate_unknown_group_is_error() {
        let mut cfg = minimal_cfg();
        cfg.rules.push(Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["group.missing".into()],
            once: false,
        });
        let result = validate(&cfg);
        assert!(result.errors.iter().any(|e| e.contains("unknown group")));
    }

    #[test]
    fn validate_backup_action_without_config_is_error() {
        let mut cfg = minimal_cfg();
        cfg.rules.push(Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["backup.check".into()],
            once: false,
        });
        let result = validate(&cfg);
        assert!(result.errors.iter().any(|e| e.contains("backup")));
    }

    #[test]
    fn validate_tagged_backup_action_with_defined_override_is_ok() {
        use crate::config::project::{BackupConfig, BackupConfigOverride};
        let mut cfg = minimal_cfg();
        let mut backup = BackupConfig {
            server: "rsync://base.example.com/".into(),
            overrides: HashMap::new(),
        };
        backup.overrides.insert(
            "offsite".into(),
            BackupConfigOverride {
                server: Some("rsync://offsite.example.com/".into()),
            },
        );
        cfg.config.backup = Some(backup);
        cfg.rules.push(Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["backup.upload#offsite".into()],
            once: false,
        });
        let result = validate(&cfg);
        assert!(
            result.errors.is_empty(),
            "unexpected errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn validate_tagged_backup_action_with_missing_override_is_error() {
        use crate::config::project::BackupConfig;
        let mut cfg = minimal_cfg();
        cfg.config.backup = Some(BackupConfig {
            server: "rsync://base.example.com/".into(),
            overrides: HashMap::new(),
        });
        cfg.rules.push(Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["backup.upload#missing".into()],
            once: false,
        });
        let result = validate(&cfg);
        assert!(
            result.errors.iter().any(|e| e.contains("missing")),
            "expected error about missing tag, got: {:?}",
            result.errors
        );
    }
}
