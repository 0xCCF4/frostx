use super::{Action, ActionContext, ActionFactory, ActionKind, ActionOutcome};
use crate::config::project::{Compression, ProjectConfig};
use crate::error::FrostxError;
use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression as GzCompression;
use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};

/// Static registration of all archive actions.
pub const REGISTRY: &[(&str, ActionFactory)] = &[("archive.compress", |config, tag| {
    Ok(Box::new(TarGz::new(config, tag)))
})];

/// Create a compressed archive of the project directory, replacing it.
///
/// The project directory is archived into a single file placed next to it in
/// the parent directory. The archive contents are verified against the source
/// before the original directory is removed, so the source is never deleted
/// unless the archive is proven intact. The pipeline state is updated to
/// reflect the new path so that subsequent actions (e.g. `backup.upload`)
/// operate on the archive file.
pub struct TarGz {
    compression: Compression,
}

impl TarGz {
    /// Construct from project config, applying any override for `tag`.
    ///
    /// Falls back to gzip if no archive config is set and no override applies.
    #[must_use]
    pub fn new(config: &ProjectConfig, tag: Option<&str>) -> Self {
        Self {
            compression: config.resolve_archive(tag).compression,
        }
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
        "archive.compress"
    }
    fn kind(&self) -> ActionKind {
        ActionKind::Mutation
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        // Canonicalize so relative paths like "." resolve to an absolute form;
        // this ensures the archive lands next to the project dir, not inside it.
        let project_path = ctx.project_path.canonicalize().map_err(FrostxError::Io)?;
        let archive_path = self.archive_path(&project_path, &ctx.config.id);

        if ctx.dry_run {
            return Ok(ActionOutcome::dry_run(format!(
                "would archive {} → {}",
                project_path.display(),
                archive_path.display()
            )));
        }

        if super::cwd_is_inside(&project_path) {
            return Err(FrostxError::ActionFailed {
                action: self.name().to_owned(),
                message: format!(
                    "current working directory is inside {}; cd to a different location and retry",
                    project_path.display()
                ),
            });
        }

        let parent = archive_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        // Build in a temp dir on the same filesystem so the final rename is atomic.
        let tmp_dir = tempfile::Builder::new()
            .prefix(".frostx-tmp-")
            .tempdir_in(parent)
            .map_err(FrostxError::Io)?;
        let tmp_path = tmp_dir.path().join(
            archive_path
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("archive")),
        );

        // Create archive in the temp dir.
        create_archive(&project_path, &tmp_path, &self.compression)?;

        // Verify before touching the final location.
        verify_archive(&project_path, &tmp_path, &self.compression)?;

        // Promote: rename (atomic on same fs) or copy+delete across fs boundaries.
        promote(&tmp_path, &archive_path).map_err(FrostxError::Io)?;

        // Only then delete the original files
        std::fs::remove_dir_all(&project_path)?;

        let meta = std::fs::metadata(&archive_path)?;
        Ok(ActionOutcome {
            status: crate::pipeline::ActionStatus::Ok,
            message: format!(
                "archived {} → {} ({})",
                project_path.display(),
                archive_path.display(),
                human_size(meta.len())
            ),
            new_project_path: Some(archive_path),
        })
    }
}

/// Move `src` to `dest`, falling back to copy + delete when they are on different filesystems.
fn promote(src: &Path, dest: &Path) -> std::io::Result<()> {
    match std::fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            std::fs::copy(src, dest)?;
            std::fs::remove_file(src)
        }
        Err(e) => Err(e),
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

/// Verify that every file and symlink in `src` is present in `archive_path`
/// with identical content. Returns an error if any entry is missing, has
/// mismatched content, or if the archive cannot be read.
///
/// The source directory is left untouched on any error.
fn verify_archive(
    src: &Path,
    archive_path: &Path,
    compression: &Compression,
) -> Result<(), FrostxError> {
    let prefix = src
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("project"))
        .to_os_string();

    // Collect every regular file and symlink under src (as paths relative to src).
    let mut src_entries: HashSet<PathBuf> = HashSet::new();
    for entry in walkdir::WalkDir::new(src).follow_links(false) {
        let entry = entry.map_err(|e| FrostxError::Io(e.into()))?;
        let ft = entry.file_type();
        if ft.is_file() || ft.is_symlink() {
            let rel = entry
                .path()
                .strip_prefix(src)
                .expect("walkdir entry is always inside src")
                .to_path_buf();
            src_entries.insert(rel);
        }
    }

    let file = File::open(archive_path)?;
    let seen = match compression {
        Compression::Gz => {
            let dec = flate2::read::GzDecoder::new(file);
            verify_entries(tar::Archive::new(dec), src, &prefix)?
        }
        Compression::Zstd => {
            let dec = zstd::Decoder::new(file).map_err(FrostxError::Io)?;
            verify_entries(tar::Archive::new(dec), src, &prefix)?
        }
        Compression::Xz => {
            let dec = xz2::read::XzDecoder::new(file);
            verify_entries(tar::Archive::new(dec), src, &prefix)?
        }
    };

    // Every source entry must appear in the archive.
    if let Some(missing) = src_entries.difference(&seen).next() {
        return Err(FrostxError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("archive is missing source file: {}", missing.display()),
        )));
    }

    Ok(())
}

/// Walk all entries in `archive`, compare each file/symlink against `src`, and
/// return the set of relative paths that were verified. Fails fast on the first
/// mismatch.
fn verify_entries<R: std::io::Read>(
    mut archive: tar::Archive<R>,
    src: &Path,
    prefix: &std::ffi::OsStr,
) -> Result<HashSet<PathBuf>, FrostxError> {
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for entry in archive.entries().map_err(FrostxError::Io)? {
        let mut entry = entry.map_err(FrostxError::Io)?;
        let path = entry.path().map_err(FrostxError::Io)?.into_owned();

        // Strip the top-level project-directory prefix that append_dir inserts.
        let rel = match path.strip_prefix(prefix) {
            Ok(r) if !r.as_os_str().is_empty() => r.to_path_buf(),
            _ => continue,
        };

        let entry_type = entry.header().entry_type();

        if entry_type.is_dir() {
            continue;
        }

        let src_path = src.join(&rel);

        if entry_type.is_symlink() {
            let archived_target = entry
                .header()
                .link_name()
                .map_err(FrostxError::Io)?
                .map(std::borrow::Cow::into_owned)
                .unwrap_or_default();
            let src_target = std::fs::read_link(&src_path).map_err(|e| {
                FrostxError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "cannot read symlink {} during verification: {e}",
                        src_path.display()
                    ),
                ))
            })?;
            if archived_target != src_target {
                return Err(FrostxError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "symlink target mismatch for entry '{}': archive='{}' source='{}'",
                        path.display(),
                        archived_target.display(),
                        src_target.display()
                    ),
                )));
            }
            seen.insert(rel);
        } else if entry_type.is_file() {
            let src_file = File::open(&src_path).map_err(|e| {
                FrostxError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "source file not accessible during verification of entry '{}': {e}",
                        path.display()
                    ),
                ))
            })?;
            let mut src_reader = std::io::BufReader::new(src_file);
            if !streams_equal(&mut src_reader, &mut entry).map_err(FrostxError::Io)? {
                return Err(FrostxError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "content mismatch: archive entry '{}' differs from source file '{}'",
                        path.display(),
                        src_path.display()
                    ),
                )));
            }
            seen.insert(rel);
        }
        // Other entry types (hard links, char/block devices, fifos, extended
        // headers) are not compared — they are uncommon in project directories.
    }

    Ok(seen)
}

/// Return `true` if both readers yield identical byte sequences.
fn streams_equal<R1: std::io::Read, R2: std::io::Read>(
    r1: &mut R1,
    r2: &mut R2,
) -> std::io::Result<bool> {
    let mut buf1 = vec![0u8; 64 * 1024];
    let mut buf2 = vec![0u8; 64 * 1024];
    loop {
        let n1 = read_filling(&mut *r1, &mut buf1)?;
        let n2 = read_filling(&mut *r2, &mut buf2)?;
        if n1 != n2 || buf1[..n1] != buf2[..n2] {
            return Ok(false);
        }
        if n1 == 0 {
            return Ok(true);
        }
    }
}

/// Read up to `buf.len()` bytes, retrying on `Interrupted`, stopping at EOF.
/// Returns the number of bytes read (0 means EOF).
fn read_filling<R: std::io::Read>(r: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match r.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(total)
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
            template: std::collections::HashMap::new(),
            groups: HashMap::new(),
            config: ActionConfig {
                archive: Some(ArchiveConfig {
                    compression,
                    overrides: std::collections::HashMap::new(),
                }),
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
        let action = TarGz::new(&cfg, None);
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
        let action = TarGz::new(&cfg, None);
        let ctx = ActionContext {
            project_path: src.path(),
            config: &cfg,
            dry_run: true,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();
        assert_eq!(out.status, crate::pipeline::ActionStatus::DryRun);
        // Source must not be deleted in dry-run mode.
        assert!(src.path().exists());
        assert!(out.new_project_path.is_none());
    }

    #[test]
    fn run_replaces_project_dir_with_archive() {
        // Create a parent dir and inside it a project sub-dir (simulates a real
        // project layout where the archive lands next to the project folder).
        let parent = tempdir().unwrap();
        let project = parent.path().join("myproject");
        std::fs::create_dir(&project).unwrap();
        std::fs::write(project.join("main.rs"), "fn main() {}").unwrap();

        let cfg = make_config(Compression::Gz);
        let action = TarGz::new(&cfg, None);
        let ctx = ActionContext {
            project_path: &project,
            config: &cfg,
            dry_run: false,
            yes: true,
        };
        let out = action.run(&ctx).unwrap();

        assert_eq!(out.status, crate::pipeline::ActionStatus::Ok);
        // Original directory must be gone.
        assert!(!project.exists(), "original project dir must be removed");
        // Archive must exist and be the returned new path.
        let new_path = out.new_project_path.expect("new_project_path must be set");
        assert!(new_path.exists(), "archive file must exist");
        assert!(new_path.metadata().unwrap().len() > 0);
    }

    #[test]
    fn verify_archive_passes_for_correct_archive() {
        let parent = tempdir().unwrap();
        let src = parent.path().join("myproject");
        let archive_path = parent.path().join("myproject.tar.gz");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("a.txt"), "hello").unwrap();
        std::fs::create_dir(src.join("sub")).unwrap();
        std::fs::write(src.join("sub").join("b.txt"), "world").unwrap();

        create_archive(&src, &archive_path, &Compression::Gz).unwrap();
        verify_archive(&src, &archive_path, &Compression::Gz)
            .expect("verification must pass for a correct archive");
    }

    #[test]
    fn verify_archive_detects_content_mismatch() {
        let parent = tempdir().unwrap();
        let src = parent.path().join("myproject");
        let archive_path = parent.path().join("myproject.tar.gz");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("data.txt"), "original").unwrap();

        create_archive(&src, &archive_path, &Compression::Gz).unwrap();

        // Tamper with the source after archiving.
        std::fs::write(src.join("data.txt"), "tampered").unwrap();

        let result = verify_archive(&src, &archive_path, &Compression::Gz);
        assert!(
            result.is_err(),
            "verification must fail when source was modified after archiving"
        );
    }

    #[test]
    fn verify_archive_detects_missing_file() {
        let parent = tempdir().unwrap();
        let src = parent.path().join("myproject");
        let archive_path = parent.path().join("myproject.tar.gz");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("a.txt"), "hello").unwrap();

        create_archive(&src, &archive_path, &Compression::Gz).unwrap();

        // Add a file to source after archiving.
        std::fs::write(src.join("extra.txt"), "not archived").unwrap();

        let result = verify_archive(&src, &archive_path, &Compression::Gz);
        assert!(
            result.is_err(),
            "verification must fail when source has a file not in the archive"
        );
    }

    #[test]
    fn failed_verification_removes_archive_and_preserves_source() {
        let parent = tempdir().unwrap();
        let project = parent.path().join("myproject");
        std::fs::create_dir(&project).unwrap();
        std::fs::write(project.join("data.txt"), "original").unwrap();

        let cfg = make_config(Compression::Gz);
        let action = TarGz::new(&cfg, None);
        let archive_path = action.archive_path(&project, &cfg.id);

        create_archive(&project, &archive_path, &Compression::Gz).unwrap();

        // Tamper with source so verification fails.
        std::fs::write(project.join("data.txt"), "tampered").unwrap();

        let result = verify_archive(&project, &archive_path, &Compression::Gz);
        assert!(result.is_err());

        // Simulate what run() does on failure: remove the archive.
        let _ = std::fs::remove_file(&archive_path);

        assert!(!archive_path.exists(), "corrupted archive must be removed");
        assert!(project.exists(), "source directory must survive");
    }
}
