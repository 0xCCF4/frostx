use super::{Action, ActionContext, ActionKind, ActionOutcome};
use crate::config::project::{Compression, ProjectConfig};
use crate::error::FrostxError;
use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression as GzCompression;
use std::fs::File;
use std::path::{Path, PathBuf};

/// Create a compressed archive of the project directory.
pub struct TarGz {
    compression: Compression,
}

impl TarGz {
    /// Construct from project config, falling back to gzip if no archive config is set.
    #[must_use]
    pub fn new(config: &ProjectConfig) -> Self {
        let compression = config
            .config
            .archive
            .as_ref()
            .map_or(Compression::Gz, |a| a.compression.clone());
        Self { compression }
    }

    /// Returns the output path for the archive.
    #[must_use]
    pub fn archive_path(&self, project_path: &Path, uuid: &uuid::Uuid) -> PathBuf {
        let parent = project_path.parent().unwrap_or(Path::new("."));
        let name = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");
        let date = Utc::now().format("%Y%m%d");
        let ext = self.compression.extension();
        parent.join(format!("{name}-{uuid}-{date}.{ext}"))
    }
}

impl Action for TarGz {
    fn name(&self) -> &'static str {
        "archive.tar_gz"
    }
    fn kind(&self) -> ActionKind {
        ActionKind::Mutation
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        let archive_path = self.archive_path(ctx.project_path, &ctx.config.id);

        if ctx.dry_run {
            return Ok(ActionOutcome::dry_run(format!(
                "would create {}",
                archive_path.display()
            )));
        }

        create_archive(ctx.project_path, &archive_path, &self.compression)?;

        let meta = std::fs::metadata(&archive_path)?;
        Ok(ActionOutcome::ok(format!(
            "created {} ({})",
            archive_path.display(),
            human_size(meta.len())
        )))
    }
}

fn create_archive(src: &Path, dest: &Path, compression: &Compression) -> Result<(), FrostxError> {
    let file = File::create(dest)?;
    match compression {
        Compression::Gz => {
            let enc = GzEncoder::new(file, GzCompression::best());
            append_dir(enc, src)?;
        }
        Compression::Zstd => {
            let enc = zstd::Encoder::new(file, 0).map_err(FrostxError::Io)?;
            let enc = append_dir(enc, src)?;
            enc.finish().map_err(FrostxError::Io)?;
        }
        Compression::Xz => {
            use xz2::write::XzEncoder;
            let enc = XzEncoder::new(file, 6);
            append_dir(enc, src)?;
        }
    }
    Ok(())
}

fn append_dir<W: std::io::Write>(writer: W, src: &Path) -> Result<W, FrostxError> {
    let mut builder = tar::Builder::new(writer);
    builder.follow_symlinks(false);
    let name = src
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("project"));
    builder.append_dir_all(name, src).map_err(FrostxError::Io)?;
    builder.into_inner().map_err(FrostxError::Io)
}

#[allow(clippy::cast_precision_loss)]
fn human_size(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::project::{ActionConfig, ArchiveConfig};
    use std::collections::HashMap;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn make_config(compression: Compression) -> crate::config::project::ProjectConfig {
        crate::config::project::ProjectConfig {
            id: Uuid::new_v4(),
            name: None,
            description: None,
            include: vec![],
            groups: HashMap::new(),
            config: ActionConfig {
                archive: Some(ArchiveConfig { compression }),
                ..ActionConfig::default()
            },
            rules: vec![],
        }
    }

    #[test]
    fn gz_archive_created() {
        let src = tempdir().unwrap();
        let out_dir = tempdir().unwrap();
        std::fs::write(src.path().join("hello.txt"), "world").unwrap();

        let cfg = make_config(Compression::Gz);
        let action = TarGz::new(&cfg);
        let archive = action.archive_path(src.path(), &cfg.id);
        // Place archive in a writable location.
        let archive = out_dir.path().join(archive.file_name().unwrap());

        create_archive(src.path(), &archive, &Compression::Gz).unwrap();
        assert!(archive.exists());
        assert!(archive.metadata().unwrap().len() > 0);
    }

    #[test]
    fn zstd_archive_created() {
        let src = tempdir().unwrap();
        let out_dir = tempdir().unwrap();
        std::fs::write(src.path().join("data.bin"), vec![0u8; 1024]).unwrap();

        let uuid = Uuid::new_v4();
        let archive = out_dir.path().join(format!("project-{uuid}-test.tar.zst"));
        create_archive(src.path(), &archive, &Compression::Zstd).unwrap();
        assert!(archive.exists());
    }

    #[test]
    fn dry_run_creates_no_file() {
        let src = tempdir().unwrap();
        std::fs::write(src.path().join("f.txt"), "x").unwrap();

        let cfg = make_config(Compression::Gz);
        let action = TarGz::new(&cfg);
        let ctx = ActionContext {
            project_path: src.path(),
            config: &cfg,
            dry_run: true,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
    }
}
