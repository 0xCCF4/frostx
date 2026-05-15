//! `frostx init` - initialize a new project directory.

use crate::config;
use crate::config::project::{ActionConfig, NotifyConfig, ProjectConfig, Rule};
use crate::error::FrostxError;
use crate::output::InitOutput;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

use super::FrostxOpts;

/// Arguments for the `init` operation.
pub struct InitArgs {
    /// Directory to initialize.
    pub path: std::path::PathBuf,
    /// Library entries or paths to prepend to the `include` list.
    pub includes: Vec<String>,
    /// Optional human-readable project name.
    pub name: Option<String>,
    /// Optional project description.
    pub description: Option<String>,
    /// Template variable values for `{{key}}` substitution in included files.
    pub template: HashMap<String, String>,
    /// Assign a new UUID to an existing project without replacing the rest of
    /// its configuration. When the target directory has no `frostx.toml` yet,
    /// behaves identically to a fresh `init`. Any `--name`, `--description`,
    /// `--include`, or `--template` flags passed alongside `--force` are applied
    /// on top of the existing config.
    pub force: bool,
}

/// Initialize a new frostx project by creating `frostx.toml` in `args.path`.
///
/// # Errors
///
/// Returns an error if the directory already contains `frostx.toml` and `--force`
/// was not set, or if the file cannot be written.
pub fn execute(args: &InitArgs, opts: &FrostxOpts) -> Result<InitOutput, FrostxError> {
    let path = &args.path;
    std::fs::create_dir_all(path)?;

    let config_path = config::config_path(path);
    if config_path.exists() && !args.force {
        return Err(FrostxError::AlreadyInitialized);
    }

    if args.force && config_path.exists() {
        return reinitialize(args, opts, path);
    }

    let uuid = Uuid::new_v4();
    let cfg = ProjectConfig {
        id: uuid,
        name: args.name.clone(),
        description: args.description.clone(),
        include: args.includes.clone(),
        template: args.template.clone(),
        groups: HashMap::new(),
        config: if args.includes.is_empty() {
            default_config()
        } else {
            ActionConfig::default()
        },
        rules: if args.includes.is_empty() {
            default_rules()
        } else {
            Vec::new()
        },
    };

    config::write_initial(path, &cfg)?;

    Ok(InitOutput {
        path: path.clone(),
        uuid,
    })
}

/// Re-initialize an existing project: assign a new UUID and apply any flag
/// overrides, preserving all other configuration.
///
/// # Errors
///
/// Returns an error if the existing config cannot be read or parsed, or if the
/// updated config cannot be written.
fn reinitialize(
    args: &InitArgs,
    opts: &FrostxOpts,
    path: &std::path::Path,
) -> Result<InitOutput, FrostxError> {
    let config_path = config::config_path(path);
    let raw = std::fs::read_to_string(&config_path)?;
    let mut cfg: ProjectConfig = toml::from_str(&raw)
        .map_err(|e| FrostxError::Config(format!("failed to parse existing config: {e}")))?;

    let old_uuid = cfg.id;
    let new_uuid = Uuid::new_v4();
    cfg.id = new_uuid;

    if let Some(ref name) = args.name {
        cfg.name = Some(name.clone());
    }
    if let Some(ref description) = args.description {
        cfg.description = Some(description.clone());
    }
    if !args.includes.is_empty() {
        cfg.include.clone_from(&args.includes);
    }
    for (k, v) in &args.template {
        cfg.template.insert(k.clone(), v.clone());
    }

    config::write_initial(path, &cfg)?;

    if old_uuid != new_uuid {
        let _ = crate::config::state::ProjectState::delete(&opts.state_dir, old_uuid);
    }

    Ok(InitOutput {
        path: path.to_path_buf(),
        uuid: new_uuid,
    })
}

fn default_config() -> ActionConfig {
    let mut result = ActionConfig::default();
    result.notifies.insert("review_project".to_string(), NotifyConfig {
        message: "This project has been idle for 6 month. You may archive it.\n\nYou may setup automatic archiving.".to_string()
    });
    result
}

fn default_rules() -> Vec<Rule> {
    use crate::config::duration::{Duration, DurationUnit};
    vec![
        Rule {
            name: Some("check state synchronized with vcs".to_string()),
            after: Duration {
                value: 3,
                unit: DurationUnit::Months,
            },
            actions: vec!["vcs.check_clean".into(), "vcs.check_pushed".into()],
        },
        Rule {
            name: Some("notify review".into()),
            after: Duration {
                value: 6,
                unit: DurationUnit::Months,
            },
            actions: vec!["notify.review_project".into()],
        },
    ]
}

/// Load a project config from `path` (or from the config override in `opts`).
///
/// # Errors
///
/// Returns an error if the config file is missing or cannot be parsed.
pub fn load_config(path: &Path, opts: &FrostxOpts) -> Result<ProjectConfig, FrostxError> {
    let dir = if let Some(ref override_path) = opts.config_override {
        override_path.parent().unwrap_or(path).to_path_buf()
    } else {
        path.to_path_buf()
    };
    config::load(&dir, &opts.library_dir)
}

/// Verify that the state file's recorded path matches `current_path`.
///
/// Returns `Err(FrostxError::UuidCollision)` if the paths differ, which
/// indicates the project directory was copied from another tracked project.
///
/// # Errors
///
/// Returns [`crate::error::FrostxError::UuidCollision`] on path mismatch, or an
/// I/O error if the state file cannot be read.
pub fn check_uuid_collision(
    config: &ProjectConfig,
    current_path: &Path,
    state_dir: &Path,
) -> Result<(), FrostxError> {
    let state = crate::config::state::ProjectState::load(state_dir, config.id)?;
    if state.project_path.as_os_str().is_empty() {
        return Ok(());
    }
    let canonical_current = current_path
        .canonicalize()
        .unwrap_or_else(|_| current_path.to_path_buf());
    let canonical_recorded = state
        .project_path
        .canonicalize()
        .unwrap_or_else(|_| state.project_path.clone());
    if canonical_current != canonical_recorded {
        return Err(FrostxError::UuidCollision {
            current: canonical_current,
            recorded: canonical_recorded,
        });
    }
    Ok(())
}
