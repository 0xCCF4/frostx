use crate::error::FrostxError;
use crate::output::human;
use chrono::{DateTime, Utc};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::Path;
use walkdir::WalkDir;

/// Filename of the per-project ignore file (gitignore syntax).
pub const IGNORE_FILENAME: &str = ".frostxignore";

/// Result of scanning a project directory for inactivity.
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// Timestamp of the most recently modified file in the project directory.
    pub last_modified: DateTime<Utc>,
    /// Number of files visited during the scan.
    #[allow(dead_code)]
    pub file_count: u64,
}

impl ScanResult {
    /// Seconds elapsed since the most recently modified file.
    #[must_use]
    pub fn inactive_seconds(&self) -> i64 {
        (Utc::now() - self.last_modified).num_seconds().max(0)
    }

    /// Human-readable inactivity description, e.g. `"97 days"`.
    #[must_use]
    #[allow(dead_code)]
    pub fn inactive_display(&self) -> String {
        human::format_seconds_as_str(self.inactive_seconds())
    }
}

/// Load a [`Gitignore`] matcher from `.frostxignore` at `dir`, if present.
///
/// Returns an empty (no-op) matcher when the file does not exist.
fn load_frostxignore(dir: &Path) -> Result<Gitignore, FrostxError> {
    let mut builder = GitignoreBuilder::new(dir);
    let ignore_path = dir.join(IGNORE_FILENAME);
    if ignore_path.exists() {
        if let Some(err) = builder.add(&ignore_path) {
            return Err(FrostxError::Config(format!(
                "failed to parse {IGNORE_FILENAME}: {err}"
            )));
        }
    }
    builder
        .build()
        .map_err(|e| FrostxError::Config(format!("failed to build ignore matcher: {e}")))
}

/// Walk `dir` recursively and return the modification time of the most
/// recently changed file.
///
/// If `dir` is a regular file (e.g. a compressed archive produced by
/// `archive.compress`), the file's own modification time is returned
/// directly without walking.
///
/// The following paths are excluded from directory scans so that frostx
/// internals and VCS bookkeeping do not reset the inactivity clock:
///
/// - `.git/` and `.jj/` directories (VCS metadata).
/// - `frostx.toml` and `.frostxignore` files.
/// - Any path matched by `.frostxignore` at the project root (full gitignore
///   syntax).
///
/// # Errors
///
/// Returns an error if the directory cannot be walked, `.frostxignore` cannot
/// be parsed, or file metadata cannot be read.
pub fn scan(dir: &Path) -> Result<ScanResult, FrostxError> {
    let meta = std::fs::metadata(dir)?;
    if meta.is_file() {
        let last_modified: DateTime<Utc> = meta
            .modified()
            .map_or(DateTime::<Utc>::MIN_UTC, DateTime::from);
        return Ok(ScanResult {
            last_modified,
            file_count: 1,
        });
    }

    let gitignore = load_frostxignore(dir)?;
    let mut latest: Option<DateTime<Utc>> = None;
    let mut file_count: u64 = 0;

    for entry in WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Never filter the root itself.
            if e.depth() == 0 {
                return true;
            }

            let name = e.file_name().to_str().unwrap_or("");
            let is_dir = e.file_type().is_dir();

            // Exclude VCS metadata directories.
            if is_dir && (name == ".git" || name == ".jj") {
                return false;
            }

            // Exclude frostx config and ignore files.
            if !is_dir && (name == crate::config::CONFIG_FILENAME || name == IGNORE_FILENAME) {
                return false;
            }

            // Apply .frostxignore patterns.
            !matches!(
                gitignore.matched(e.path(), is_dir),
                ignore::Match::Ignore(_)
            )
        })
    {
        let entry = entry.map_err(|e| FrostxError::Io(e.into()))?;
        let entry_meta = entry.metadata().map_err(|e| FrostxError::Io(e.into()))?;
        if !entry_meta.is_file() {
            continue;
        }
        file_count += 1;

        let modified: DateTime<Utc> = entry_meta
            .modified()
            .map_or(DateTime::<Utc>::MIN_UTC, DateTime::from);

        latest = Some(match latest {
            Some(prev) if modified > prev => modified,
            Some(prev) => prev,
            None => modified,
        });
    }

    Ok(ScanResult {
        last_modified: latest.unwrap_or_else(Utc::now),
        file_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scan_detects_recent_file() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("hello.txt"), "hi").unwrap();
        let result = scan(tmp.path()).unwrap();
        assert!(result.inactive_seconds() < 60);
        assert_eq!(result.file_count, 1);
    }

    #[test]
    fn scan_empty_dir() {
        let tmp = tempdir().unwrap();
        let result = scan(tmp.path()).unwrap();
        assert_eq!(result.file_count, 0);
    }

    #[test]
    fn scan_skips_config_file() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join(crate::config::CONFIG_FILENAME), "[...]").unwrap();
        let result = scan(tmp.path()).unwrap();
        assert_eq!(result.file_count, 0);
    }

    #[test]
    fn scan_skips_frostxignore_file() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join(IGNORE_FILENAME), "dist/").unwrap();
        let result = scan(tmp.path()).unwrap();
        assert_eq!(result.file_count, 0);
    }

    #[test]
    fn scan_skips_git_dir() {
        let tmp = tempdir().unwrap();
        let git = tmp.path().join(".git");
        fs::create_dir(&git).unwrap();
        fs::write(git.join("HEAD"), "ref: refs/heads/main").unwrap();
        let result = scan(tmp.path()).unwrap();
        assert_eq!(result.file_count, 0);
    }

    #[test]
    fn scan_skips_jj_dir() {
        let tmp = tempdir().unwrap();
        let jj = tmp.path().join(".jj");
        fs::create_dir(&jj).unwrap();
        fs::write(jj.join("repo"), "jj state").unwrap();
        let result = scan(tmp.path()).unwrap();
        assert_eq!(result.file_count, 0);
    }

    #[test]
    fn scan_frostxignore_excludes_matched_dir() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join(IGNORE_FILENAME), "dist/\n").unwrap();
        let dist = tmp.path().join("dist");
        fs::create_dir(&dist).unwrap();
        fs::write(dist.join("bundle.js"), "code").unwrap();
        // dist/ should be ignored; only the real source file counts.
        fs::write(tmp.path().join("main.rs"), "fn main() {}").unwrap();
        let result = scan(tmp.path()).unwrap();
        assert_eq!(result.file_count, 1);
    }

    #[test]
    fn scan_frostxignore_excludes_matched_file() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join(IGNORE_FILENAME), "*.log\n").unwrap();
        fs::write(tmp.path().join("debug.log"), "log content").unwrap();
        fs::write(tmp.path().join("main.rs"), "fn main() {}").unwrap();
        let result = scan(tmp.path()).unwrap();
        assert_eq!(result.file_count, 1);
    }

    #[test]
    fn scan_no_frostxignore_file_is_fine() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("main.rs"), "fn main() {}").unwrap();
        let result = scan(tmp.path()).unwrap();
        assert_eq!(result.file_count, 1);
    }

    #[test]
    fn scan_git_dir_newer_mtime_not_counted() {
        let tmp = tempdir().unwrap();
        // Write a user file first.
        let user_file = tmp.path().join("code.rs");
        fs::write(&user_file, "fn main() {}").unwrap();
        // .git dir with a newer file — should not influence last_modified.
        let git = tmp.path().join(".git");
        fs::create_dir(&git).unwrap();
        fs::write(git.join("FETCH_HEAD"), "abc123").unwrap();

        let result = scan(tmp.path()).unwrap();
        assert_eq!(result.file_count, 1);
        let user_mtime: DateTime<Utc> =
            fs::metadata(&user_file).unwrap().modified().unwrap().into();
        assert_eq!(result.last_modified, user_mtime);
    }

    #[test]
    fn inactive_display_minutes() {
        let result = ScanResult {
            last_modified: Utc::now() - chrono::Duration::minutes(10),
            file_count: 1,
        };
        assert!(result.inactive_display().contains("minutes"));
    }

    #[test]
    fn inactive_display_hours() {
        let result = ScanResult {
            last_modified: Utc::now() - chrono::Duration::hours(3),
            file_count: 1,
        };
        assert!(result.inactive_display().contains("hours"));
    }

    #[test]
    fn inactive_display_days() {
        let result = ScanResult {
            last_modified: Utc::now() - chrono::Duration::days(97),
            file_count: 1,
        };
        assert!(result.inactive_display().contains("days"));
    }

    #[test]
    fn scan_archive_file_uses_file_mtime() {
        let tmp = tempdir().unwrap();
        let archive = tmp.path().join("project.tar.gz");
        fs::write(&archive, b"fake archive content").unwrap();
        let result = scan(&archive).unwrap();
        assert!(result.inactive_seconds() < 60);
        assert_eq!(result.file_count, 1);
    }
}
