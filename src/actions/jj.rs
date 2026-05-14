use super::{Action, ActionContext, ActionKind, ActionOutcome};
use crate::error::FrostxError;
use chrono::Utc;
use std::process::Command;

fn is_jj_repo(path: &std::path::Path) -> bool {
    path.join(".jj").exists()
}

fn jj(args: &[&str], dir: &std::path::Path) -> std::io::Result<std::process::Output> {
    Command::new("jj").args(args).current_dir(dir).output()
}

/// Check that the working copy has no uncommitted changes.
pub struct CheckClean;

impl Action for CheckClean {
    fn name(&self) -> &'static str {
        "jj.check_clean"
    }

    fn kind(&self) -> ActionKind {
        ActionKind::Check
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        if !is_jj_repo(ctx.project_path) {
            return Ok(ActionOutcome::skipped("not a jj repository"));
        }
        let out = jj(&["diff", "--summary"], ctx.project_path)?;
        if out.stdout.is_empty() {
            Ok(ActionOutcome::ok("working copy is clean"))
        } else {
            let detail = String::from_utf8_lossy(&out.stdout).trim().to_string();
            Ok(ActionOutcome::failed(format!(
                "working copy has changes:\n{detail}"
            )))
        }
    }
}

/// Fetch from all remotes then check for unpushed commits.
pub struct CheckPushed;

impl Action for CheckPushed {
    fn name(&self) -> &'static str {
        "jj.check_pushed"
    }

    fn kind(&self) -> ActionKind {
        ActionKind::Check
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        if !is_jj_repo(ctx.project_path) {
            return Ok(ActionOutcome::skipped("not a jj repository"));
        }
        // Auto-fetch so comparisons are against current remote state.
        let _ = jj(&["git", "fetch"], ctx.project_path);

        // List ancestors of @ (excluding @) that are not reachable from any remote bookmark.
        // An empty result means everything is pushed.
        let out = jj(
            &[
                "log",
                "--no-graph",
                "-r",
                "remote_bookmarks()..@-",
                "-T",
                "commit_id ++ \"\\n\"",
            ],
            ctx.project_path,
        );
        match out {
            Ok(o) if o.stdout.is_empty() => Ok(ActionOutcome::ok("all commits pushed")),
            Ok(o) => {
                let detail = String::from_utf8_lossy(&o.stdout).trim().to_string();
                Ok(ActionOutcome::failed(format!(
                    "unpushed commits:\n{detail}"
                )))
            }
            Err(_) => {
                // No remote configured - not an error.
                Ok(ActionOutcome::ok("no remote configured"))
            }
        }
    }
}

/// Create a bookmark at `@` marking the last active state.
pub struct Bookmark;

impl Action for Bookmark {
    fn name(&self) -> &'static str {
        "jj.bookmark"
    }

    fn kind(&self) -> ActionKind {
        ActionKind::Mutation
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        if !is_jj_repo(ctx.project_path) {
            return Ok(ActionOutcome::skipped("not a jj repository"));
        }
        let name = format!("frostx-archive-{}", Utc::now().format("%Y%m%d"));

        if ctx.dry_run {
            return Ok(ActionOutcome::dry_run(format!(
                "would create bookmark {name}"
            )));
        }

        let out = jj(&["bookmark", "create", &name, "-r", "@-"], ctx.project_path)?;

        if out.status.success() {
            Ok(ActionOutcome::ok(format!("created bookmark {name}")))
        } else {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            // Bookmark already exists is not a hard error.
            if err.contains("already exists") {
                Ok(ActionOutcome::ok(format!("bookmark {name} already exists")))
            } else {
                Ok(ActionOutcome::failed(format!("jj bookmark failed: {err}")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::project::ActionConfig;
    use std::collections::HashMap;
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
            include: vec![],
            groups: HashMap::new(),
            config: ActionConfig::default(),
            rules: vec![],
        }
    }

    #[test]
    fn check_clean_non_jj_skips() {
        let tmp = tempdir().unwrap();
        let cfg = make_config();
        let ctx = make_ctx(tmp.path(), &cfg);
        let out = CheckClean.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Skipped);
    }

    #[test]
    fn check_pushed_non_jj_skips() {
        let tmp = tempdir().unwrap();
        let cfg = make_config();
        let ctx = make_ctx(tmp.path(), &cfg);
        let out = CheckPushed.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Skipped);
    }

    #[test]
    fn bookmark_non_jj_skips() {
        let tmp = tempdir().unwrap();
        let cfg = make_config();
        let ctx = make_ctx(tmp.path(), &cfg);
        let out = Bookmark.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Skipped);
    }

    #[test]
    fn bookmark_dry_run() {
        let tmp = tempdir().unwrap();
        // Simulate a jj repo by creating the .jj directory.
        std::fs::create_dir(tmp.path().join(".jj")).unwrap();
        let cfg = make_config();
        let mut ctx = make_ctx(tmp.path(), &cfg);
        ctx.dry_run = true;
        let out = Bookmark.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
    }
}
