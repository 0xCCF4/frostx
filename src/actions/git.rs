use super::{Action, ActionContext, ActionKind, ActionOutcome};
use crate::error::FrostxError;
use chrono::Utc;
use std::process::Command;

fn is_git_repo(path: &std::path::Path) -> bool {
    path.join(".git").exists()
}

fn git(args: &[&str], dir: &std::path::Path) -> std::io::Result<std::process::Output> {
    Command::new("git").args(args).current_dir(dir).output()
}

/// Check that the working tree has no uncommitted changes.
pub struct CheckClean;

impl Action for CheckClean {
    fn name(&self) -> &'static str {
        "git.check_clean"
    }
    fn kind(&self) -> ActionKind {
        ActionKind::Check
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        if !is_git_repo(ctx.project_path) {
            return Ok(ActionOutcome::skipped("not a git repository"));
        }
        let out = git(&["status", "--porcelain"], ctx.project_path)?;
        if out.stdout.is_empty() {
            Ok(ActionOutcome::ok("working tree is clean"))
        } else {
            let detail = String::from_utf8_lossy(&out.stdout).trim().to_string();
            Ok(ActionOutcome::failed(format!(
                "uncommitted changes:\n{detail}"
            )))
        }
    }
}

/// Fetch from all remotes then check for unpushed commits.
pub struct CheckPushed;

impl Action for CheckPushed {
    fn name(&self) -> &'static str {
        "git.check_pushed"
    }
    fn kind(&self) -> ActionKind {
        ActionKind::Check
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        if !is_git_repo(ctx.project_path) {
            return Ok(ActionOutcome::skipped("not a git repository"));
        }
        // Auto-fetch so comparisons are against current remote state.
        let _ = git(&["fetch", "--all", "--quiet"], ctx.project_path);

        let out = git(&["log", "--oneline", "@{u}..HEAD"], ctx.project_path);
        match out {
            Ok(o) if o.stdout.is_empty() => Ok(ActionOutcome::ok("all commits pushed")),
            Ok(o) => {
                let detail = String::from_utf8_lossy(&o.stdout).trim().to_string();
                Ok(ActionOutcome::failed(format!(
                    "unpushed commits:\n{detail}"
                )))
            }
            Err(_) => {
                // No upstream configured - treat as ok (no remote to push to).
                Ok(ActionOutcome::ok("no remote configured"))
            }
        }
    }
}

/// Remove untracked files and directories (`git clean -fd`).
pub struct Clean;

impl Action for Clean {
    fn name(&self) -> &'static str {
        "git.clean"
    }
    fn kind(&self) -> ActionKind {
        ActionKind::Mutation
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        if !is_git_repo(ctx.project_path) {
            return Ok(ActionOutcome::skipped("not a git repository"));
        }
        // Preview first.
        let preview = git(&["clean", "-nfd"], ctx.project_path)?;
        let preview_text = String::from_utf8_lossy(&preview.stdout).trim().to_string();

        if preview_text.is_empty() {
            return Ok(ActionOutcome::ok("nothing to clean"));
        }

        if ctx.dry_run {
            return Ok(ActionOutcome::dry_run(format!(
                "would remove:\n{preview_text}"
            )));
        }

        println!("Would remove:\n{preview_text}");
        if !ctx.yes && !confirm("Remove these untracked files?")? {
            return Ok(ActionOutcome::skipped("cancelled by user"));
        }

        let out = git(&["clean", "-fd"], ctx.project_path)?;
        if out.status.success() {
            let removed = String::from_utf8_lossy(&out.stdout).trim().to_string();
            Ok(ActionOutcome::ok(format!(
                "removed untracked files:\n{removed}"
            )))
        } else {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Ok(ActionOutcome::failed(format!("git clean failed: {err}")))
        }
    }
}

/// Create an annotated git tag marking the last active state.
pub struct Tag;

impl Action for Tag {
    fn name(&self) -> &'static str {
        "git.tag"
    }
    fn kind(&self) -> ActionKind {
        ActionKind::Mutation
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        if !is_git_repo(ctx.project_path) {
            return Ok(ActionOutcome::skipped("not a git repository"));
        }
        let tag = format!("frostx-archive-{}", Utc::now().format("%Y%m%d"));

        if ctx.dry_run {
            return Ok(ActionOutcome::dry_run(format!("would create tag {tag}")));
        }

        let out = git(
            &[
                "tag",
                "-a",
                &tag,
                "-m",
                &format!("frostx archive checkpoint {}", Utc::now().to_rfc3339()),
            ],
            ctx.project_path,
        )?;

        if out.status.success() {
            Ok(ActionOutcome::ok(format!("created tag {tag}")))
        } else {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            // Tag already exists is not a hard error.
            if err.contains("already exists") {
                Ok(ActionOutcome::ok(format!("tag {tag} already exists")))
            } else {
                Ok(ActionOutcome::failed(format!("git tag failed: {err}")))
            }
        }
    }
}

fn confirm(prompt: &str) -> Result<bool, FrostxError> {
    use dialoguer::Confirm;
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
    use std::process::Command;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn make_ctx<'a>(
        path: &'a std::path::Path,
        config: &'a crate::config::project::ProjectConfig,
    ) -> ActionContext<'a> {
        ActionContext {
            project_path: path,
            config,
            dry_run: false,
            yes: true,
        }
    }

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

    fn init_git(dir: &std::path::Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn check_clean_non_git_skips() {
        let tmp = tempdir().unwrap();
        let cfg = make_config();
        let ctx = make_ctx(tmp.path(), &cfg);
        let out = CheckClean.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Skipped);
    }

    #[test]
    fn check_clean_clean_repo() {
        let tmp = tempdir().unwrap();
        init_git(tmp.path());
        // Initial commit so HEAD exists.
        std::fs::write(tmp.path().join("README.md"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let cfg = make_config();
        let ctx = make_ctx(tmp.path(), &cfg);
        let out = CheckClean.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
    }

    #[test]
    fn check_clean_dirty_repo() {
        let tmp = tempdir().unwrap();
        init_git(tmp.path());
        std::fs::write(tmp.path().join("dirty.txt"), "change").unwrap();

        let cfg = make_config();
        let ctx = make_ctx(tmp.path(), &cfg);
        let out = CheckClean.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Failed);
    }

    #[test]
    fn git_tag_dry_run() {
        let tmp = tempdir().unwrap();
        init_git(tmp.path());
        std::fs::write(tmp.path().join("f.txt"), "x").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let cfg = make_config();
        let mut ctx = make_ctx(tmp.path(), &cfg);
        ctx.dry_run = true;
        let out = Tag.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
    }
}
