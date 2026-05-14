use super::{Action, ActionContext, ActionFactory, ActionKind, ActionOutcome};
use crate::error::FrostxError;

/// Static registration of all local actions.
pub const REGISTRY: &[(&str, ActionFactory)] = &[("local.delete", |_| Ok(Box::new(Delete)))];

/// Delete the local project directory. Always asks for explicit confirmation.
pub struct Delete;

impl Action for Delete {
    fn name(&self) -> &'static str {
        "local.delete"
    }
    fn kind(&self) -> ActionKind {
        ActionKind::Mutation
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        let path = ctx.project_path;

        if ctx.dry_run {
            return Ok(ActionOutcome::dry_run(format!(
                "would delete {}",
                path.display()
            )));
        }

        let size: u64 = walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter_map(|e| e.metadata().ok())
            .filter(std::fs::Metadata::is_file)
            .map(|m| m.len())
            .sum();

        // local.delete ALWAYS confirms, even with --yes.
        println!(
            "About to permanently delete: {} ({} on disk)",
            path.display(),
            human_size(size)
        );
        if !confirm_delete(path)? {
            return Ok(ActionOutcome::skipped("cancelled by user"));
        }

        std::fs::remove_dir_all(path)?;
        Ok(ActionOutcome::ok(format!("deleted {}", path.display())))
    }
}

#[allow(clippy::cast_precision_loss)]
fn human_size(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn confirm_delete(path: &std::path::Path) -> Result<bool, FrostxError> {
    use dialoguer::Confirm;
    let prompt = format!(
        "Permanently delete {}? This cannot be undone",
        path.display()
    );
    Confirm::new()
        .with_prompt(prompt)
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
    fn dry_run_does_not_delete() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("myproject");
        std::fs::create_dir(&project).unwrap();
        std::fs::write(project.join("main.rs"), "fn main() {}").unwrap();

        let cfg = make_config();
        let ctx = ActionContext {
            project_path: &project,
            config: &cfg,
            dry_run: true,
            yes: true,
        };
        let out = Delete.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
        assert!(project.exists());
    }
}
