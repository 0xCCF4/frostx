use super::project::{ActionConfig, Group, ProjectConfig, Rule};
use crate::error::FrostxError;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Source of TOML fragment content for include resolution.
enum FragmentSource {
    /// Read from the filesystem at the given path.
    File(PathBuf),
    /// Content was already buffered from an in-memory map (archive case).
    Memory { key: String, content: String },
    /// Relative path that cannot be resolved (e.g. `../` from inside archive).
    Unresolvable {
        source: String,
        reason: &'static str,
    },
}

/// Extract all `{{variable_name}}` placeholders from `content`.
///
/// Only names consisting of ASCII alphanumeric characters or underscores are
/// recognized as valid template variables. Duplicates are removed; order of
/// first appearance is preserved.
#[must_use]
pub fn extract_template_vars(content: &str) -> Vec<String> {
    let mut vars: Vec<String> = Vec::new();
    let mut pos = 0;
    while let Some(rel) = content[pos..].find("{{") {
        let inner_start = pos + rel + 2;
        match content[inner_start..].find("}}") {
            None => break,
            Some(inner_len) => {
                let name = content[inner_start..inner_start + inner_len].trim();
                if !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !vars.iter().any(|v| v == name)
                {
                    vars.push(name.to_owned());
                }
                pos = inner_start + inner_len + 2;
            }
        }
    }
    vars
}

/// Resolve all `include` entries in `base`, merge them in, and return the final config.
///
/// `project_dir` is the directory containing `frostx.toml`; relative includes
/// (starting with `./` or `../`) are resolved against it.
///
/// Include order: entries are applied left-to-right; local values always win.
///
/// # Errors
///
/// Returns an error if any included file cannot be read or parsed.
pub fn resolve_includes(
    base: ProjectConfig,
    project_dir: &Path,
    library_dir: &Path,
) -> Result<ProjectConfig, FrostxError> {
    resolve_includes_impl(base, |source| {
        if source.starts_with('/') {
            FragmentSource::File(PathBuf::from(source))
        } else if source.starts_with("./") || source.starts_with("../") {
            FragmentSource::File(project_dir.join(source))
        } else {
            FragmentSource::File(library_dir.join(format!("{source}.toml")))
        }
    })
}

/// Resolve all `include` entries in `base` where relative includes are
/// satisfied from the in-memory TOML map produced by reading an archive.
///
/// Relative paths (`./` or `../`) are looked up in `archive_entries`; library
/// and absolute includes are still resolved from the filesystem.  Relative
/// `../` includes that would escape the project directory cannot be in the
/// archive and are reported as errors.
///
/// # Errors
///
/// Returns an error if any included file cannot be found or parsed.
pub fn resolve_includes_from_archive<S: std::hash::BuildHasher>(
    base: ProjectConfig,
    library_dir: &Path,
    archive_entries: &HashMap<String, String, S>,
) -> Result<ProjectConfig, FrostxError> {
    resolve_includes_impl(base, |source| {
        if source.starts_with('/') {
            FragmentSource::File(PathBuf::from(source))
        } else if source.starts_with("../") {
            FragmentSource::Unresolvable {
                source: source.to_owned(),
                reason:
                    "relative includes escaping the project root are not available in an archive",
            }
        } else if let Some(rel) = source.strip_prefix("./") {
            // Look the file up inside the in-memory archive map.
            if let Some(content) = archive_entries.get(rel) {
                FragmentSource::Memory {
                    key: rel.to_owned(),
                    content: content.clone(),
                }
            } else {
                FragmentSource::Unresolvable {
                    source: source.to_owned(),
                    reason: "file not found inside the archive",
                }
            }
        } else {
            // Bare library name.
            FragmentSource::File(library_dir.join(format!("{source}.toml")))
        }
    })
}

fn resolve_includes_impl(
    base: ProjectConfig,
    resolve: impl Fn(&str) -> FragmentSource,
) -> Result<ProjectConfig, FrostxError> {
    if base.include.is_empty() {
        return Ok(base);
    }
    let includes = base.include.clone();
    let mut merged_rules: Vec<Rule> = Vec::new();
    let mut merged_groups: HashMap<String, Group> = HashMap::new();
    let mut merged_config = ActionConfig::default();
    let mut seen: HashSet<String> = HashSet::new();

    for source in &includes {
        let fragment_source = resolve(source);

        let dedup_key = match &fragment_source {
            FragmentSource::File(path) => Some(path.to_string_lossy().into_owned()),
            FragmentSource::Memory { key, .. } => Some(key.clone()),
            FragmentSource::Unresolvable { .. } => None,
        };
        if let Some(key) = dedup_key {
            if !seen.insert(key) {
                continue;
            }
        }

        let fragment = match fragment_source {
            FragmentSource::File(path) => {
                load_fragment(&path, &base.template).map_err(|e| FrostxError::Include {
                    path: source.clone(),
                    message: e.to_string(),
                })?
            }
            FragmentSource::Memory { key, content } => {
                load_fragment_from_str(&content, &key, &base.template).map_err(|e| {
                    FrostxError::Include {
                        path: source.clone(),
                        message: e.to_string(),
                    }
                })?
            }
            FragmentSource::Unresolvable {
                source: src,
                reason,
            } => {
                return Err(FrostxError::Include {
                    path: src,
                    message: reason.to_owned(),
                });
            }
        };

        // Included rules are prepended (collected here, merged below in order).
        merged_rules.extend(fragment.rules);
        // Included groups: later includes and local win.
        for (k, v) in fragment.groups {
            merged_groups.entry(k).or_insert(v);
        }
        // Included config: first-seen wins (local override applied at the end).
        merge_config(&mut merged_config, fragment.config);
    }

    // Append local rules after included ones.
    merged_rules.extend(base.rules);
    // Local groups override included ones.
    for (k, v) in base.groups {
        merged_groups.insert(k, v);
    }
    // Local config overrides included config.
    merge_config_local(&mut merged_config, base.config);

    Ok(ProjectConfig {
        id: base.id,
        name: base.name,
        description: base.description,
        include: base.include,
        template: base.template,
        groups: merged_groups,
        config: merged_config,
        rules: merged_rules,
    })
}

/// A partial config that may appear in an included file (no `id`, no nested `include`).
#[derive(Debug, serde::Deserialize)]
struct Fragment {
    #[serde(default, rename = "group")]
    groups: HashMap<String, Group>,
    #[serde(default)]
    config: ActionConfig,
    #[serde(default, rename = "rule")]
    rules: Vec<Rule>,
}

fn load_fragment(path: &Path, template: &HashMap<String, String>) -> Result<Fragment, FrostxError> {
    let raw = std::fs::read_to_string(path)?;
    let content = apply_template(&raw, template, path)?;
    toml::from_str(&content)
        .map_err(|e| FrostxError::Config(crate::diagnostics::format_toml_error(&e, path)))
}

fn load_fragment_from_str(
    raw: &str,
    display_name: &str,
    template: &HashMap<String, String>,
) -> Result<Fragment, FrostxError> {
    let path = std::path::Path::new(display_name);
    let content = apply_template(raw, template, path)?;
    toml::from_str(&content)
        .map_err(|e| FrostxError::Config(crate::diagnostics::format_toml_error(&e, path)))
}

/// Replace every `{{key}}` occurrence in `raw` with the corresponding value
/// from `template`. Returns an error if any placeholder remains unresolved.
fn apply_template(
    raw: &str,
    template: &HashMap<String, String>,
    path: &Path,
) -> Result<String, FrostxError> {
    let mut content = raw.to_owned();
    for (key, value) in template {
        let placeholder = "{{".to_owned() + key + "}}";
        content = content.replace(&placeholder, value);
    }
    // Detect unresolved placeholders.
    if let Some(start) = content.find("{{") {
        let after = &content[start + 2..];
        let name = after
            .find("}}")
            .map_or("<unknown>", |end| after[..end].trim());
        return Err(FrostxError::Config(format!(
            "{}: unresolved template variable `{{{{{name}}}}}` - add it to the `[template]` section of frostx.toml",
            path.display(),
        )));
    }
    Ok(content)
}

/// Merge `src` into `dst`, with `dst` taking precedence (first-write-wins).
fn merge_config(dst: &mut ActionConfig, src: ActionConfig) {
    if dst.backup.is_none() {
        dst.backup = src.backup;
    }
    if dst.archive.is_none() {
        dst.archive = src.archive;
    }
    if dst.fs.is_none() {
        dst.fs = src.fs;
    }
    if dst.vcs.is_none() {
        dst.vcs = src.vcs;
    }
    for (k, v) in src.hooks {
        dst.hooks.entry(k).or_insert(v);
    }
    for (k, v) in src.notifies {
        dst.notifies.entry(k).or_insert(v);
    }
}

/// Apply local config on top - local values always win.
fn merge_config_local(dst: &mut ActionConfig, local: ActionConfig) {
    if local.backup.is_some() {
        dst.backup = local.backup;
    }
    if local.archive.is_some() {
        dst.archive = local.archive;
    }
    if local.fs.is_some() {
        dst.fs = local.fs;
    }
    if local.vcs.is_some() {
        dst.vcs = local.vcs;
    }
    for (k, v) in local.hooks {
        dst.hooks.insert(k, v);
    }
    for (k, v) in local.notifies {
        dst.notifies.insert(k, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::duration::Duration;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn base_config() -> ProjectConfig {
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
    fn no_includes_is_noop() {
        let cfg = base_config();
        let tmp = tempdir().unwrap();
        let result = resolve_includes(cfg.clone(), tmp.path(), tmp.path()).unwrap();
        assert_eq!(result.rules.len(), 0);
    }

    #[test]
    fn library_include_merges_rules() {
        let tmp = tempdir().unwrap();
        let lib_file = tmp.path().join("my-template.toml");
        std::fs::write(
            &lib_file,
            r#"
[[rule]]
after = "30d"
actions = ["git.check_clean"]
"#,
        )
        .unwrap();

        let mut cfg = base_config();
        cfg.include = vec!["my-template".into()];
        cfg.rules.push(super::super::project::Rule {
            name: None,
            after: Duration::parse("90d").unwrap(),
            actions: vec!["git.check_pushed".into()],
            once: false,
        });

        // lib dir = project dir (both tmp) so "my-template" resolves to tmp/my-template.toml
        let result = resolve_includes(cfg, tmp.path(), tmp.path()).unwrap();
        // Included rule comes first.
        assert_eq!(result.rules[0].actions, vec!["git.check_clean"]);
        assert_eq!(result.rules[1].actions, vec!["git.check_pushed"]);
    }

    #[test]
    fn relative_include_resolves_against_project_dir() {
        let project_dir = tempdir().unwrap();
        let lib_dir = tempdir().unwrap();
        // Write the fragment relative to the project dir.
        std::fs::write(
            project_dir.path().join("shared.toml"),
            r#"
[[rule]]
after = "14d"
actions = ["git.check_clean"]
"#,
        )
        .unwrap();

        let mut cfg = base_config();
        cfg.include = vec!["./shared.toml".into()];

        let result = resolve_includes(cfg, project_dir.path(), lib_dir.path()).unwrap();
        assert_eq!(result.rules.len(), 1);
        assert_eq!(result.rules[0].actions, vec!["git.check_clean"]);
    }

    #[test]
    fn local_config_overrides_included() {
        let tmp = tempdir().unwrap();
        let lib_file = tmp.path().join("base.toml");
        std::fs::write(
            &lib_file,
            r#"
[config.backup]
server = "rsync://included-server/"
"#,
        )
        .unwrap();

        let mut cfg = base_config();
        cfg.include = vec!["base".into()];
        cfg.config.backup = Some(super::super::project::BackupConfig {
            server: "rsync://local-server/".into(),
        });

        let result = resolve_includes(cfg, tmp.path(), tmp.path()).unwrap();
        assert_eq!(
            result.config.backup.unwrap().server,
            "rsync://local-server/"
        );
    }

    #[test]
    fn duplicate_includes_are_skipped() {
        let tmp = tempdir().unwrap();
        std::fs::write(
            tmp.path().join("shared.toml"),
            "[[rule]]\nafter = \"30d\"\nactions = [\"git.check_clean\"]\n",
        )
        .unwrap();

        let mut cfg = base_config();
        cfg.include = vec!["shared".into(), "shared".into()];

        let result = resolve_includes(cfg, tmp.path(), tmp.path()).unwrap();
        assert_eq!(
            result.rules.len(),
            1,
            "duplicate include should not double the rules"
        );
    }

    #[test]
    fn missing_include_is_error() {
        let tmp = tempdir().unwrap();
        let mut cfg = base_config();
        cfg.include = vec!["nonexistent".into()];
        assert!(resolve_includes(cfg, tmp.path(), tmp.path()).is_err());
    }

    #[test]
    fn extract_vars_empty() {
        assert!(extract_template_vars("no placeholders here").is_empty());
    }

    #[test]
    fn extract_vars_single() {
        let vars = extract_template_vars("server = \"{{backup_server}}\"");
        assert_eq!(vars, vec!["backup_server"]);
    }

    #[test]
    fn extract_vars_deduplicates() {
        let vars = extract_template_vars("{{x}} and {{x}} again");
        assert_eq!(vars, vec!["x"]);
    }

    #[test]
    fn extract_vars_ignores_invalid_names() {
        let vars = extract_template_vars("{{ has spaces }} and {{valid_name}}");
        assert_eq!(vars, vec!["valid_name"]);
    }

    #[test]
    fn template_substitution_in_include() {
        let tmp = tempdir().unwrap();
        std::fs::write(
            tmp.path().join("tpl.toml"),
            r#"
[config.backup]
server = "{{backup_server}}"
"#,
        )
        .unwrap();

        let mut cfg = base_config();
        cfg.include = vec!["tpl".into()];
        cfg.template
            .insert("backup_server".into(), "rsync://example.com/".into());

        let result = resolve_includes(cfg, tmp.path(), tmp.path()).unwrap();
        assert_eq!(result.config.backup.unwrap().server, "rsync://example.com/");
    }

    #[test]
    fn unresolved_template_var_is_error() {
        let tmp = tempdir().unwrap();
        std::fs::write(
            tmp.path().join("tpl.toml"),
            r#"
[config.backup]
server = "{{missing_var}}"
"#,
        )
        .unwrap();

        let mut cfg = base_config();
        cfg.include = vec!["tpl".into()];
        // No template values provided - should fail.

        let err = resolve_includes(cfg, tmp.path(), tmp.path()).unwrap_err();
        assert!(
            err.to_string().contains("missing_var"),
            "error should mention the variable name: {err}"
        );
    }

    #[test]
    fn archive_relative_include_resolved_from_memory() {
        let lib = tempdir().unwrap();
        let mut cfg = base_config();
        cfg.include = vec!["./extra.toml".into()];

        let mut archive_entries = HashMap::new();
        archive_entries.insert(
            "extra.toml".into(),
            "[[rule]]\nafter = \"30d\"\nactions = []\n".into(),
        );

        let result = resolve_includes_from_archive(cfg, lib.path(), &archive_entries).unwrap();
        assert_eq!(result.rules.len(), 1);
    }

    #[test]
    fn archive_missing_relative_include_is_error() {
        let lib = tempdir().unwrap();
        let mut cfg = base_config();
        cfg.include = vec!["./missing.toml".into()];

        let archive_entries: HashMap<String, String> = HashMap::new();

        assert!(resolve_includes_from_archive(cfg, lib.path(), &archive_entries).is_err());
    }

    #[test]
    fn archive_dotdot_include_is_error() {
        let lib = tempdir().unwrap();
        let mut cfg = base_config();
        cfg.include = vec!["../outside.toml".into()];

        let archive_entries: HashMap<String, String> = HashMap::new();

        let err = resolve_includes_from_archive(cfg, lib.path(), &archive_entries).unwrap_err();
        assert!(err.to_string().contains("outside"), "error: {err}");
    }

    #[test]
    fn archive_library_include_resolved_from_filesystem() {
        let lib = tempdir().unwrap();
        std::fs::write(
            lib.path().join("mylib.toml"),
            "[[rule]]\nafter = \"7d\"\nactions = []\n",
        )
        .unwrap();

        let mut cfg = base_config();
        cfg.include = vec!["mylib".into()];

        let archive_entries: HashMap<String, String> = HashMap::new();

        let result = resolve_includes_from_archive(cfg, lib.path(), &archive_entries).unwrap();
        assert_eq!(result.rules.len(), 1);
    }

    #[test]
    fn archive_include_toml_syntax_error_fails_nicely() {
        let lib = tempdir().unwrap();
        let mut cfg = base_config();
        cfg.include = vec!["./bad.toml".into()];

        let mut archive_entries = HashMap::new();
        archive_entries.insert("bad.toml".into(), "this is [not valid toml".into());

        let err = resolve_includes_from_archive(cfg, lib.path(), &archive_entries).unwrap_err();
        // Should report the file name and a parse error, not "not found".
        assert!(err.to_string().contains("bad.toml"), "error: {err}");
    }
}
