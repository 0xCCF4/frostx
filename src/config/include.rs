use super::project::{ActionConfig, Group, ProjectConfig, Rule};
use crate::error::FrostxError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    if base.include.is_empty() {
        return Ok(base);
    }
    let includes = base.include.clone();
    let mut merged_rules: Vec<Rule> = Vec::new();
    let mut merged_groups: HashMap<String, Group> = HashMap::new();
    let mut merged_config = ActionConfig::default();

    for source in &includes {
        let path = resolve_source(source, project_dir, library_dir);
        let fragment = load_fragment(&path, &base.template).map_err(|e| FrostxError::Include {
            path: source.clone(),
            message: e.to_string(),
        })?;
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

fn resolve_source(source: &str, project_dir: &Path, library_dir: &Path) -> PathBuf {
    if source.starts_with('/') {
        PathBuf::from(source)
    } else if source.starts_with("./") || source.starts_with("../") {
        project_dir.join(source)
    } else {
        // Bare name - library lookup.
        library_dir.join(format!("{source}.toml"))
    }
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
}
