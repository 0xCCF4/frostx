//! User notification action - displays a message and requires explicit confirmation.

use super::{Action, ActionContext, ActionKind, ActionOutcome};
use crate::config::project::NotifyConfig;
use crate::error::FrostxError;

/// Pause the pipeline, display a message, and require explicit user confirmation
/// before allowing execution to continue.
///
/// Always prompts regardless of `--yes`, because the intent is to ensure a human
/// has reviewed the situation.
pub struct Notify {
    config: NotifyConfig,
}

impl Notify {
    /// Construct from a notify config entry.
    #[must_use]
    pub fn new(config: NotifyConfig) -> Self {
        Self { config }
    }
}

impl Action for Notify {
    fn name(&self) -> &'static str {
        "notify"
    }

    fn kind(&self) -> ActionKind {
        ActionKind::Mutation
    }

    fn supports_compressed_archive(&self) -> bool {
        true
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        if ctx.dry_run {
            return Ok(ActionOutcome::dry_run(format!(
                "would display: {}",
                self.config.message
            )));
        }

        println!("\n{}", self.config.message);

        if !confirm_proceed()? {
            return Ok(ActionOutcome::skipped("cancelled by user"));
        }

        Ok(ActionOutcome::ok("confirmed"))
    }
}

fn confirm_proceed() -> Result<bool, FrostxError> {
    use dialoguer::Confirm;
    Confirm::new()
        .with_prompt("Proceed?")
        .default(false)
        .interact()
        .map_err(|e| FrostxError::Other(e.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::project::ActionConfig;
    use std::collections::HashMap;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn make_config() -> crate::config::project::ProjectConfig {
        crate::config::project::ProjectConfig {
            id: Uuid::new_v4(),
            name: None,
            description: None,
            include: vec![],
            template: std::collections::HashMap::new(),
            groups: HashMap::new(),
            config: ActionConfig::default(),
            rules: vec![],
        }
    }

    #[test]
    fn dry_run_returns_dry_run_status() {
        let tmp = tempdir().unwrap();
        let cfg = make_config();
        let action = Notify::new(NotifyConfig {
            message: "Review the checklist before continuing.".into(),
        });
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: true,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
        assert!(out.message.contains("Review the checklist"));
    }

    #[test]
    fn kind_is_mutation() {
        let action = Notify::new(NotifyConfig {
            message: "hello".into(),
        });
        assert_eq!(action.kind(), ActionKind::Mutation);
    }
}
