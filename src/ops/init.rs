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
    /// Overwrite an existing `frostx.toml` and assign a new UUID.
    pub force: bool,
}

/// Initialize a new frostx project by creating `frostx.toml` in `args.path`.
pub fn execute(args: &InitArgs, opts: &FrostxOpts) -> Result<InitOutput, FrostxError> {
    let path = &args.path;
    std::fs::create_dir_all(path)?;

    let config_path = config::config_path(path);
    if config_path.exists() && !args.force {
        return Err(FrostxError::AlreadyInitialized);
    }

    // When --force reinitializes an existing project, capture the old UUID so we
    // can delete its now-orphaned state file after writing the new config.
    let old_uuid: Option<Uuid> = if args.force && config_path.exists() {
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| toml::from_str::<ProjectConfig>(&s).ok())
            .map(|c| c.id)
    } else {
        None
    };

    let uuid = Uuid::new_v4();
    let cfg = ProjectConfig {
        id: uuid,
        include: args.includes.clone(),
        groups: HashMap::new(),
        config: default_config(),
        rules: default_rules(),
    };

    config::write_initial(path, &cfg)?;

    if let Some(old) = old_uuid {
        if old != uuid {
            let _ = crate::config::state::ProjectState::delete(&opts.state_dir, old);
        }
    }

    Ok(InitOutput {
        path: path.clone(),
        uuid,
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
    vec![Rule {
        after: Duration {
            value: 3,
            unit: DurationUnit::Months,
        },
        actions: vec!["vcs.check_clean".into(), "vcs.check_pushed".into()],
    }, Rule {
        after: Duration {
            value: 6,
            unit: DurationUnit::Months,
        },
        actions: vec!["notify.review_project".into()],
    }]
}

/// Load a project config from `path` (or from the config override in `opts`).
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
