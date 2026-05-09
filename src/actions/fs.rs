use super::{Action, ActionContext, ActionKind, ActionOutcome};
use crate::config::project::ProjectConfig;
use crate::error::FrostxError;
use std::path::PathBuf;

/// Delete common build artifact directories before archiving.
pub struct CleanArtifacts {
    targets: Vec<String>,
}

impl CleanArtifacts {
    /// Construct from project config, using the default artifact list if `[config.fs]` is absent.
    pub fn new(config: &ProjectConfig) -> Self {
        let targets = config.config.fs.as_ref().map_or_else(
            crate::config::project::FsConfig::default_clean_artifacts,
            |f| f.clean_artifacts.clone(),
        );
        Self { targets }
    }

    fn find_targets(&self, project_path: &std::path::Path) -> Vec<(PathBuf, u64)> {
        self.targets
            .iter()
            .filter_map(|t| {
                let p = project_path.join(t.trim_end_matches('/'));
                if p.exists() {
                    let size = dir_size(&p);
                    Some((p, size))
                } else {
                    None
                }
            })
            .collect()
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

fn dir_size(path: &std::path::Path) -> u64 {
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
    use crate::config::project::{ActionConfig, FsConfig};
    use std::collections::HashMap;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn make_config_with_targets(targets: Vec<String>) -> crate::config::project::ProjectConfig {
        crate::config::project::ProjectConfig {
            id: Uuid::new_v4(),
            include: vec![],
            groups: HashMap::new(),
            config: ActionConfig {
                fs: Some(FsConfig {
                    clean_artifacts: targets,
                }),
                ..ActionConfig::default()
            },
            rules: vec![],
        }
    }

    #[test]
    fn no_artifacts_is_ok() {
        let tmp = tempdir().unwrap();
        let cfg = make_config_with_targets(vec!["node_modules/".into()]);
        let action = CleanArtifacts::new(&cfg);
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
    fn removes_artifact_dir_with_yes() {
        let tmp = tempdir().unwrap();
        let artifact = tmp.path().join("node_modules");
        std::fs::create_dir(&artifact).unwrap();
        std::fs::write(artifact.join("pkg.js"), "data").unwrap();

        let cfg = make_config_with_targets(vec!["node_modules/".into()]);
        let action = CleanArtifacts::new(&cfg);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
        assert!(!artifact.exists());
    }

    #[test]
    fn dry_run_does_not_remove() {
        let tmp = tempdir().unwrap();
        let artifact = tmp.path().join("target");
        std::fs::create_dir(&artifact).unwrap();

        let cfg = make_config_with_targets(vec!["target/".into()]);
        let action = CleanArtifacts::new(&cfg);
        let ctx = ActionContext {
            project_path: tmp.path(),
            config: &cfg,
            dry_run: true,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
        assert!(artifact.exists());
    }

    #[test]
    fn human_size_formats() {
        assert_eq!(human_size(500), "500 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert!(human_size(2 * 1024 * 1024).contains("MB"));
    }
}
