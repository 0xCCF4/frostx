use crate::error::FrostxError;
use crate::output::human;
use chrono::{DateTime, Utc};
use std::path::Path;
use walkdir::WalkDir;

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

/// Walk `dir` recursively and return the modification time of the most
/// recently changed file.
///
/// If `dir` is a regular file (e.g. a compressed archive produced by
/// `archive.compress`), the file's own modification time is returned
/// directly without walking.
///
/// The `frostx.toml` file is excluded from directory scans so that
/// `frostx run` does not reset the inactivity clock.
///
/// # Errors
///
/// Returns an error if the directory cannot be walked or file metadata cannot be read.
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

    let mut latest: Option<DateTime<Utc>> = None;
    let mut file_count: u64 = 0;

    for entry in WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|e| FrostxError::Io(e.into()))?;
        let path = entry.path();

        // Skip the config file - its mtime must not influence inactivity.
        if path.file_name().and_then(|n| n.to_str()) == Some(crate::config::CONFIG_FILENAME) {
            continue;
        }

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
        last_modified: latest.unwrap_or_else(Utc::now), // if there are no files
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
        // A file just written should be very recent.
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
        // Only write frostx.toml - it should not count.
        fs::write(tmp.path().join(crate::config::CONFIG_FILENAME), "[...]").unwrap();
        let result = scan(tmp.path()).unwrap();
        assert_eq!(result.file_count, 0);
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
        // The archive is brand-new, so it should be very recent.
        assert!(result.inactive_seconds() < 60);
        assert_eq!(result.file_count, 1);
    }
}
