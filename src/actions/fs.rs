use super::{Action, ActionContext, ActionFactory, ActionKind, ActionOutcome};
use crate::config::project::ProjectConfig;
use crate::error::FrostxError;
use std::path::{Path, PathBuf};

/// Static registration of all fs actions.
pub const REGISTRY: &[(&str, ActionFactory)] = &[("fs.clean_artifacts", |config, tag| {
    Ok(Box::new(CleanArtifacts::new(config, tag)))
})];

/// Detects a project type and returns the artifact paths to remove.
///
/// Each implementation checks for a language-specific marker file before returning
/// any paths, so no artifacts are touched when the project type is not detected.
pub(crate) trait Cleaner: Send + Sync {
    /// Returns existing artifact paths under `project_path` for a detected project type.
    /// Returns an empty vec if the project type is not detected or no artifacts exist.
    fn artifacts(&self, project_path: &Path) -> Vec<PathBuf>;
}

struct RustCleaner;

impl Cleaner for RustCleaner {
    fn artifacts(&self, project_path: &Path) -> Vec<PathBuf> {
        if !project_path.join("Cargo.toml").exists() {
            return vec![];
        }
        let p = project_path.join("target");
        if p.exists() {
            vec![p]
        } else {
            vec![]
        }
    }
}

struct NodeCleaner;

impl Cleaner for NodeCleaner {
    fn artifacts(&self, project_path: &Path) -> Vec<PathBuf> {
        if !project_path.join("package.json").exists() {
            return vec![];
        }
        let p = project_path.join("node_modules");
        if p.exists() {
            vec![p]
        } else {
            vec![]
        }
    }
}

struct PythonCleaner;

impl Cleaner for PythonCleaner {
    fn artifacts(&self, project_path: &Path) -> Vec<PathBuf> {
        let detected =
            project_path.join("pyproject.toml").exists() || project_path.join("setup.py").exists();
        if !detected {
            return vec![];
        }

        let mut paths = Vec::new();

        let venv = project_path.join(".venv");
        if venv.exists() {
            paths.push(venv);
        }

        // Collect __pycache__ dirs, but skip inside .venv to avoid double-counting.
        let venv_prefix = project_path.join(".venv");
        for entry in walkdir::WalkDir::new(project_path)
            .into_iter()
            .filter_entry(|e| e.path() != venv_prefix)
            .filter_map(std::result::Result::ok)
        {
            if entry.file_type().is_dir() && entry.file_name() == "__pycache__" {
                paths.push(entry.into_path());
            }
        }

        paths
    }
}

/// Delete build artifact directories before archiving.
pub struct CleanArtifacts {
    cleaners: Vec<Box<dyn Cleaner>>,
    extra_paths: Vec<String>,
}

impl CleanArtifacts {
    /// Construct from project config, applying any override for `tag`.
    #[must_use]
    pub fn new(config: &ProjectConfig, tag: Option<&str>) -> Self {
        let resolved = config.resolve_fs(tag);

        let mut cleaners: Vec<Box<dyn Cleaner>> = Vec::new();
        if resolved.cleaners.rust {
            cleaners.push(Box::new(RustCleaner));
        }
        if resolved.cleaners.node {
            cleaners.push(Box::new(NodeCleaner));
        }
        if resolved.cleaners.python {
            cleaners.push(Box::new(PythonCleaner));
        }

        Self {
            cleaners,
            extra_paths: resolved.extra_paths,
        }
    }

    fn find_targets(&self, project_path: &Path) -> Vec<(PathBuf, u64)> {
        let mut targets = Vec::new();

        for cleaner in &self.cleaners {
            for path in cleaner.artifacts(project_path) {
                let size = dir_size(&path);
                targets.push((path, size));
            }
        }

        for extra in &self.extra_paths {
            let p = project_path.join(extra.trim_end_matches('/'));
            if p.exists() {
                let size = dir_size(&p);
                targets.push((p, size));
            }
        }

        targets
    }
}

impl Action for CleanArtifacts {
    fn name(&self) -> &'static str {
        "fs.clean_artifacts"
    }

    fn kind(&self) -> ActionKind {
        ActionKind::Mutation
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        let found = self.find_targets(ctx.project_path);

        if found.is_empty() {
            return Ok(ActionOutcome::ok("no artifact directories found"));
        }

        let summary: Vec<String> = found
            .iter()
            .map(|(p, sz)| format!("  {} ({})", p.display(), human_size(*sz)))
            .collect();
        let summary_str = summary.join("\n");

        if ctx.dry_run {
            return Ok(ActionOutcome::dry_run(format!(
                "would remove:\n{summary_str}"
            )));
        }

        println!("Artifact directories to remove:\n{summary_str}");
        if !ctx.yes && !confirm("Remove these directories?")? {
            return Ok(ActionOutcome::skipped("cancelled by user"));
        }

        let mut removed = 0u64;
        let mut errors = Vec::new();
        for (path, sz) in &found {
            match std::fs::remove_dir_all(path) {
                Ok(()) => removed += sz,
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }

        if errors.is_empty() {
            Ok(ActionOutcome::ok(format!(
                "removed {} ({})",
                found.len(),
                human_size(removed)
            )))
        } else {
            Ok(ActionOutcome::failed(format!(
                "some removals failed:\n{}",
                errors.join("\n")
            )))
        }
    }
}

fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter_map(|e| e.metadata().ok())
        .filter(std::fs::Metadata::is_file)
        .map(|m| m.len())
        .sum()
}

#[allow(clippy::cast_precision_loss)]
fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
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
    use crate::config::project::{ActionConfig, CleanersConfig, FsConfig};
    use std::collections::HashMap;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn make_config(fs: Option<FsConfig>) -> crate::config::project::ProjectConfig {
        crate::config::project::ProjectConfig {
            id: Uuid::new_v4(),
            name: None,
            description: None,
            include: vec![],
            template: HashMap::new(),
            groups: HashMap::new(),
            config: ActionConfig {
                fs,
                ..ActionConfig::default()
            },
            rules: vec![],
        }
    }

    fn default_config() -> crate::config::project::ProjectConfig {
        make_config(None)
    }

    #[test]
    fn no_marker_no_artifacts() {
        let tmp = tempdir().unwrap();
        let cfg = default_config();
        let action = CleanArtifacts::new(&cfg, None);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
        assert!(out.message.contains("no artifact directories"));
    }

    #[test]
    fn rust_cleaner_removes_target_when_cargo_toml_present() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
        let target = tmp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("binary"), "data").unwrap();

        let cfg = default_config();
        let action = CleanArtifacts::new(&cfg, None);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
        assert!(!target.exists());
    }

    #[test]
    fn rust_cleaner_skips_target_without_cargo_toml() {
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("target");
        std::fs::create_dir(&target).unwrap();

        let cfg = default_config();
        let action = CleanArtifacts::new(&cfg, None);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
        assert!(out.message.contains("no artifact directories"));
        assert!(target.exists());
    }

    #[test]
    fn node_cleaner_removes_node_modules() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("package.json"), "{}").unwrap();
        let nm = tmp.path().join("node_modules");
        std::fs::create_dir(&nm).unwrap();
        std::fs::write(nm.join("pkg.js"), "data").unwrap();

        let cfg = default_config();
        let action = CleanArtifacts::new(&cfg, None);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
        assert!(!nm.exists());
    }

    #[test]
    fn python_cleaner_detects_pyproject_toml() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("pyproject.toml"), "[project]").unwrap();
        let venv = tmp.path().join(".venv");
        std::fs::create_dir(&venv).unwrap();

        let cfg = default_config();
        let action = CleanArtifacts::new(&cfg, None);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
        assert!(!venv.exists());
    }

    #[test]
    fn python_cleaner_removes_pycache_dirs() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("pyproject.toml"), "[project]").unwrap();
        let pkg = tmp.path().join("mypackage");
        std::fs::create_dir(&pkg).unwrap();
        let cache = pkg.join("__pycache__");
        std::fs::create_dir(&cache).unwrap();
        std::fs::write(cache.join("mod.cpython-312.pyc"), "bytecode").unwrap();
        let nested = pkg.join("sub");
        std::fs::create_dir(&nested).unwrap();
        let nested_cache = nested.join("__pycache__");
        std::fs::create_dir(&nested_cache).unwrap();

        let cfg = default_config();
        let action = CleanArtifacts::new(&cfg, None);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
        assert!(!cache.exists());
        assert!(!nested_cache.exists());
    }

    #[test]
    fn python_cleaner_detects_setup_py() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("setup.py"), "from setuptools import setup").unwrap();
        let venv = tmp.path().join(".venv");
        std::fs::create_dir(&venv).unwrap();

        let cfg = default_config();
        let action = CleanArtifacts::new(&cfg, None);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
        assert!(!venv.exists());
    }

    #[test]
    fn disabled_cleaner_does_not_run() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
        let target = tmp.path().join("target");
        std::fs::create_dir(&target).unwrap();

        let cfg = make_config(Some(FsConfig {
            cleaners: CleanersConfig {
                rust: false,
                node: true,
                python: true,
            },
            ..FsConfig::default()
        }));
        let action = CleanArtifacts::new(&cfg, None);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
        assert!(out.message.contains("no artifact directories"));
        assert!(target.exists());
    }

    #[test]
    fn extra_paths_removed_unconditionally() {
        let tmp = tempdir().unwrap();
        let custom = tmp.path().join("build");
        std::fs::create_dir(&custom).unwrap();

        let cfg = make_config(Some(FsConfig {
            extra_paths: vec!["build/".into()],
            cleaners: CleanersConfig {
                rust: false,
                node: false,
                python: false,
            },
            ..FsConfig::default()
        }));
        let action = CleanArtifacts::new(&cfg, None);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
        assert!(!custom.exists());
    }

    #[test]
    fn dry_run_does_not_remove() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
        let target = tmp.path().join("target");
        std::fs::create_dir(&target).unwrap();

        let cfg = default_config();
        let action = CleanArtifacts::new(&cfg, None);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: true,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
        assert!(target.exists());
    }

    #[test]
    fn human_size_formats() {
        assert_eq!(human_size(500), "500 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert!(human_size(2 * 1024 * 1024).contains("MB"));
    }
}
