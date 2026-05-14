use super::{Action, ActionContext, ActionKind, ActionOutcome};
use crate::error::FrostxError;

/// Uniform interface that each VCS backend must provide for the shared `vcs.*` actions.
///
/// To add support for a new VCS:
/// 1. Implement this trait for a new zero-size struct.
/// 2. Add it to the `backends()` list in the correct detection-priority order.
/// 3. Add an `actions/<vcs>.rs` module with the concrete action implementations.
trait VcsBackend: Send + Sync {
    /// Return `true` if this backend manages `path`.
    fn is_applicable(&self, path: &std::path::Path) -> bool;

    fn check_clean(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError>;
    fn check_pushed(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError>;
    /// Mark the last active state (annotated tag, bookmark, etc.).
    fn mark(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError>;
}

struct GitBackend;
struct JjBackend;

impl VcsBackend for GitBackend {
    fn is_applicable(&self, path: &std::path::Path) -> bool {
        path.join(".git").exists()
    }
    fn check_clean(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        super::git::CheckClean.run(ctx)
    }
    fn check_pushed(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        super::git::CheckPushed.run(ctx)
    }
    fn mark(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        super::git::Tag.run(ctx)
    }
}

impl VcsBackend for JjBackend {
    fn is_applicable(&self, path: &std::path::Path) -> bool {
        path.join(".jj").exists()
    }
    fn check_clean(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        super::jj::CheckClean.run(ctx)
    }
    fn check_pushed(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        super::jj::CheckPushed.run(ctx)
    }
    fn mark(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        super::jj::Bookmark.run(ctx)
    }
}

/// Ordered list of VCS backends.
///
/// Jj is listed before git because jj with a git backend creates both `.jj` and `.git`.
/// In that configuration jj is the active VCS and must take precedence.
fn backends() -> Vec<Box<dyn VcsBackend>> {
    vec![Box::new(JjBackend), Box::new(GitBackend)]
}

fn find_backend(path: &std::path::Path) -> Option<Box<dyn VcsBackend>> {
    backends().into_iter().find(|b| b.is_applicable(path))
}

/// Returns the outcome for when no VCS is detected.
///
/// Fails by default. Skips silently only when `[config.vcs] skip_if_no_vcs = true`.
fn no_vcs_outcome(ctx: &ActionContext<'_>) -> ActionOutcome {
    let skip = ctx
        .config
        .config
        .vcs
        .as_ref()
        .is_some_and(|v| v.skip_if_no_vcs);
    if skip {
        ActionOutcome::skipped("no VCS repository detected")
    } else {
        ActionOutcome::failed(
            "no VCS repository detected (set [config.vcs] skip_if_no_vcs = true to skip instead)",
        )
    }
}

/// Auto-detect VCS and check for uncommitted changes.
pub struct CheckClean;

impl Action for CheckClean {
    fn name(&self) -> &'static str {
        "vcs.check_clean"
    }

    fn kind(&self) -> ActionKind {
        ActionKind::Check
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        match find_backend(ctx.project_path) {
            Some(b) => b.check_clean(ctx),
            None => Ok(no_vcs_outcome(ctx)),
        }
    }
}

/// Auto-detect VCS, fetch from remotes, and check for unpushed commits.
pub struct CheckPushed;

impl Action for CheckPushed {
    fn name(&self) -> &'static str {
        "vcs.check_pushed"
    }

    fn kind(&self) -> ActionKind {
        ActionKind::Check
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        match find_backend(ctx.project_path) {
            Some(b) => b.check_pushed(ctx),
            None => Ok(no_vcs_outcome(ctx)),
        }
    }
}

/// Auto-detect VCS and mark the last active state (git annotated tag / jj bookmark).
pub struct Mark;

impl Action for Mark {
    fn name(&self) -> &'static str {
        "vcs.mark"
    }

    fn kind(&self) -> ActionKind {
        ActionKind::Mutation
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        match find_backend(ctx.project_path) {
            Some(b) => b.mark(ctx),
            None => Ok(no_vcs_outcome(ctx)),
        }
    }
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
            include: vec![],
            groups: HashMap::new(),
            config: ActionConfig::default(),
            rules: vec![],
        }
    }

    fn make_config_skip_if_no_vcs() -> crate::config::project::ProjectConfig {
        use crate::config::project::VcsConfig;
        let mut cfg = make_config();
        cfg.config.vcs = Some(VcsConfig {
            skip_if_no_vcs: true,
        });
        cfg
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
    fn check_clean_non_vcs_fails_by_default() {
        let tmp = tempdir().unwrap();
        let cfg = make_config();
        let ctx = make_ctx(tmp.path(), &cfg);
        let out = CheckClean.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Failed);
    }

    #[test]
    fn check_pushed_non_vcs_fails_by_default() {
        let tmp = tempdir().unwrap();
        let cfg = make_config();
        let ctx = make_ctx(tmp.path(), &cfg);
        let out = CheckPushed.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Failed);
    }

    #[test]
    fn mark_non_vcs_fails_by_default() {
        let tmp = tempdir().unwrap();
        let cfg = make_config();
        let ctx = make_ctx(tmp.path(), &cfg);
        let out = Mark.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Failed);
    }

    #[test]
    fn check_clean_non_vcs_skips_when_opted_in() {
        let tmp = tempdir().unwrap();
        let cfg = make_config_skip_if_no_vcs();
        let ctx = make_ctx(tmp.path(), &cfg);
        let out = CheckClean.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Skipped);
    }

    #[test]
    fn mark_non_vcs_skips_when_opted_in() {
        let tmp = tempdir().unwrap();
        let cfg = make_config_skip_if_no_vcs();
        let ctx = make_ctx(tmp.path(), &cfg);
        let out = Mark.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Skipped);
    }

    #[test]
    fn check_clean_delegates_to_git() {
        let tmp = tempdir().unwrap();
        init_git(tmp.path());
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
    fn mark_git_dry_run() {
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
        let out = Mark.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
        assert!(
            out.message.contains("tag"),
            "expected git tag message, got: {}",
            out.message
        );
    }

    #[test]
    fn mark_jj_dry_run() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".jj")).unwrap();
        let cfg = make_config();
        let mut ctx = make_ctx(tmp.path(), &cfg);
        ctx.dry_run = true;
        let out = Mark.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
        assert!(
            out.message.contains("bookmark"),
            "expected jj bookmark message, got: {}",
            out.message
        );
    }

    #[test]
    fn jj_preferred_over_git_when_both_present() {
        let tmp = tempdir().unwrap();
        // Simulate jj with git backend: both .jj and .git exist.
        std::fs::create_dir(tmp.path().join(".jj")).unwrap();
        init_git(tmp.path());
        let cfg = make_config();
        let mut ctx = make_ctx(tmp.path(), &cfg);
        ctx.dry_run = true;
        let out = Mark.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
        assert!(
            out.message.contains("bookmark"),
            "expected jj backend, got: {}",
            out.message
        );
    }
}
