use super::{Action, ActionContext, ActionKind, ActionOutcome};
use crate::config::project::{HookConfig, HookKind};
use crate::error::FrostxError;
use std::process::Command;

/// Execute a user-defined shell command.
pub struct Hook {
    config: HookConfig,
}

impl Hook {
    /// Construct from a hook config entry. The `_name` parameter is accepted for
    /// symmetry with other constructors but is not stored.
    #[must_use]
    pub fn new(_name: &str, config: HookConfig) -> Self {
        Self { config }
    }
}

impl Action for Hook {
    fn name(&self) -> &'static str {
        "hook"
    }

    fn kind(&self) -> ActionKind {
        match self.config.kind {
            HookKind::Check => ActionKind::Check,
            HookKind::Mutation => ActionKind::Mutation,
        }
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        if ctx.dry_run {
            return Ok(ActionOutcome::dry_run(format!(
                "would run: {}",
                self.config.command
            )));
        }

        let out = Command::new("sh")
            .arg("-c")
            .arg(&self.config.command)
            .current_dir(ctx.project_path)
            .env("FROSTX_PROJECT_ID", ctx.config.id.to_string())
            .env("FROSTX_DRY_RUN", if ctx.dry_run { "1" } else { "0" }) // currently, script is skipped, but might change in the future
            .env("FROSTX_YES", if ctx.yes { "1" } else { "0" })
            .env("FROSTX_PROJECT_PATH", ctx.project_path)
            .output()?;

        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

        if out.status.success() {
            let msg = if stdout.is_empty() {
                "ok".to_string()
            } else {
                stdout
            };
            Ok(ActionOutcome::ok(msg))
        } else {
            let msg = if stderr.is_empty() {
                format!("exit code {}", out.status.code().unwrap_or(-1))
            } else {
                stderr
            };
            Ok(ActionOutcome::failed(msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::project::{ActionConfig, HookConfig, HookKind};
    use std::collections::HashMap;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn make_config() -> crate::config::project::ProjectConfig {
        crate::config::project::ProjectConfig {
            id: Uuid::new_v4(),
            include: vec![],
            groups: HashMap::new(),
            config: ActionConfig::default(),
            rules: vec![],
        }
    }

    #[test]
    fn hook_success() {
        let tmp = tempdir().unwrap();
        let cfg = make_config();
        let hook = Hook::new(
            "my_hook",
            HookConfig {
                command: "echo hello".into(),
                kind: HookKind::Mutation,
            },
        );
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = hook.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
    }

    #[test]
    fn hook_failure() {
        let tmp = tempdir().unwrap();
        let cfg = make_config();
        let hook = Hook::new(
            "my_hook",
            HookConfig {
                command: "exit 1".into(),
                kind: HookKind::Mutation,
            },
        );
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = hook.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Failed);
    }

    #[test]
    fn hook_dry_run() {
        let tmp = tempdir().unwrap();
        let cfg = make_config();
        let hook = Hook::new(
            "my_hook",
            HookConfig {
                command: "rm -rf /".into(),
                kind: HookKind::Mutation,
            },
        );
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: true,
            yes: true,
        };
        let out = hook.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
    }

    #[test]
    fn check_kind_hook() {
        let hook = Hook::new(
            "verify",
            HookConfig {
                command: "true".into(),
                kind: HookKind::Check,
            },
        );
        assert_eq!(hook.kind(), ActionKind::Check);
    }

    #[test]
    fn mutation_kind_hook() {
        let hook = Hook::new(
            "build",
            HookConfig {
                command: "make".into(),
                kind: HookKind::Mutation,
            },
        );
        assert_eq!(hook.kind(), ActionKind::Mutation);
    }
}
