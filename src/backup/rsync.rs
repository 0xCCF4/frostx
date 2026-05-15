use super::{BackendFactory, BackupBackend};
use crate::error::FrostxError;
use sha2::{Digest, Sha256};
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

/// Scheme-to-factory map for this backend. Both `rsync://` and `ssh://` are
/// handled by the same [`RsyncBackend`] implementation.
pub const REGISTRY: &[(&str, BackendFactory)] = &[
    ("rsync://", |url| Box::new(RsyncBackend::new(url))),
    ("ssh://", |url| Box::new(RsyncBackend::new(url))),
];

/// Backup backend that shells out to the `rsync` binary.
pub struct RsyncBackend {
    server: String,
}

impl RsyncBackend {
    /// Construct from a server URL (`rsync://...` or `ssh://...`).
    #[must_use]
    pub fn new(server: &str) -> Self {
        Self {
            server: server.to_string(),
        }
    }

    fn remote_path(&self, uuid: Uuid) -> String {
        // Normalize trailing slash then append uuid filename.
        let base = self.server.trim_end_matches('/');
        format!("{base}/{uuid}.tar.gz")
    }

    /// Verify using `rsync --checksum --dry-run --itemize-changes`.
    ///
    /// rsync compares the local and remote file by checksum. If nothing would be
    /// transferred (itemize output contains no line ending with the archive filename),
    /// the remote copy is intact.
    fn verify_rsync(&self, uuid: Uuid, local_archive: &Path) -> Result<bool, FrostxError> {
        let remote = self.remote_path(uuid);
        let archive_str = local_archive
            .to_str()
            .ok_or_else(|| FrostxError::ActionFailed {
                action: "backup.verify".into(),
                message: "archive path contains non-UTF-8 characters".into(),
            })?;

        let out = Command::new("rsync")
            .args([
                "--dry-run",
                "--checksum",
                "--archive",
                "--itemize-changes",
                archive_str,
                &remote,
            ])
            .output()
            .map_err(|e| FrostxError::Config(format!("rsync not found: {e}")))?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(FrostxError::ActionFailed {
                action: "backup.verify".into(),
                message: format!("rsync checksum verification failed: {err}"),
            });
        }

        // --itemize-changes outputs a line per file that would be transferred.
        // If the archive filename appears, the remote differs or is missing.
        let stdout = String::from_utf8_lossy(&out.stdout);
        let file_name = local_archive
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        Ok(!stdout.lines().any(|line| line.ends_with(file_name)))
    }

    /// Verify for `ssh://` servers.
    ///
    /// Tries `ssh sha256sum <remote_path>` first and compares against the local
    /// SHA-256. Falls back to downloading the remote archive via rsync and comparing
    /// checksums locally if the remote command fails.
    fn verify_ssh(&self, uuid: Uuid, local_archive: &Path) -> Result<bool, FrostxError> {
        let local_hash = sha256_file(local_archive)?;

        if let Some(ref target) = parse_ssh_target(&self.server, uuid) {
            if let Ok(Some(remote_hash)) = try_remote_sha256sum(target) {
                return Ok(remote_hash == local_hash);
            }
            // sha256sum unavailable or SSH failed — fall through to download
        }

        // Fallback: download the remote archive to a temp file and compare checksums.
        let tmp = tempfile::NamedTempFile::new()?;
        let tmp_str = tmp
            .path()
            .to_str()
            .ok_or_else(|| FrostxError::ActionFailed {
                action: "backup.verify".into(),
                message: "temp path contains non-UTF-8 characters".into(),
            })?;
        let remote = self.remote_path(uuid);

        let out = Command::new("rsync")
            .args(["--archive", &remote, tmp_str])
            .output()
            .map_err(|e| FrostxError::Config(format!("rsync not found: {e}")))?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(FrostxError::ActionFailed {
                action: "backup.verify".into(),
                message: format!("download for verification failed: {err}"),
            });
        }

        let downloaded_hash = sha256_file(tmp.path())?;
        Ok(downloaded_hash == local_hash)
    }
}

impl BackupBackend for RsyncBackend {
    fn check(&self, uuid: Uuid) -> Result<bool, FrostxError> {
        let remote = self.remote_path(uuid);
        // rsync --list-only exits 0 if the file exists.
        let out = Command::new("rsync")
            .args(["--list-only", &remote])
            .output()
            .map_err(|e| FrostxError::Config(format!("rsync not found: {e}")))?;
        Ok(out.status.success())
    }

    fn upload(&self, uuid: Uuid, archive_path: &Path) -> Result<String, FrostxError> {
        let remote = self.remote_path(uuid);
        let out = Command::new("rsync")
            .args([
                "--archive",
                "--compress",
                "--progress",
                archive_path.to_str().unwrap_or(""),
                &remote,
            ])
            .output()
            .map_err(|e| FrostxError::Config(format!("rsync not found: {e}")))?;
        if out.status.success() {
            Ok(remote)
        } else {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(FrostxError::ActionFailed {
                action: "backup.upload".into(),
                message: err,
            })
        }
    }

    fn verify(&self, uuid: Uuid, local_archive: &Path) -> Result<bool, FrostxError> {
        if self.server.starts_with("ssh://") {
            self.verify_ssh(uuid, local_archive)
        } else {
            self.verify_rsync(uuid, local_archive)
        }
    }
}

/// Parsed components of an `ssh://` URL needed to run a remote command.
struct SshTarget {
    /// `[user@]host` portion of the URL.
    userhost: String,
    /// SSH port (default 22).
    port: u16,
    /// Absolute remote file path, e.g. `/backups/projects/<uuid>.tar.gz`.
    remote_path: String,
}

/// Parse `ssh://[user@]host[:port]/base/path` and construct the remote archive path.
///
/// Returns `None` if the URL is malformed (missing path separator).
fn parse_ssh_target(server: &str, uuid: Uuid) -> Option<SshTarget> {
    let rest = server.strip_prefix("ssh://")?;
    let slash = rest.find('/')?;
    let authority = &rest[..slash];
    let base_path = rest[slash..].trim_end_matches('/');

    let (userhost, port) = match authority.rfind(':') {
        Some(idx) if authority[idx + 1..].chars().all(|c| c.is_ascii_digit()) => {
            let port: u16 = authority[idx + 1..].parse().ok()?;
            (authority[..idx].to_string(), port)
        }
        _ => (authority.to_string(), 22),
    };

    Some(SshTarget {
        userhost,
        port,
        remote_path: format!("{base_path}/{uuid}.tar.gz"),
    })
}

/// Run `sha256sum` on the remote host via SSH.
///
/// Returns `Ok(Some(hex_hash))` on success, `Ok(None)` if the remote command
/// exited non-zero (e.g. `sha256sum` not installed), or an error if SSH itself
/// could not be invoked.
fn try_remote_sha256sum(target: &SshTarget) -> Result<Option<String>, FrostxError> {
    let port_str = target.port.to_string();
    // Single-quote the remote path to prevent shell expansion on the remote side.
    let escaped = target.remote_path.replace('\'', r"'\''");
    let remote_cmd = format!("sha256sum '{escaped}'");

    let out = Command::new("ssh")
        .args([
            "-p",
            &port_str,
            "-o",
            "BatchMode=yes",
            &target.userhost,
            &remote_cmd,
        ])
        .output()
        .map_err(|e| FrostxError::ActionFailed {
            action: "backup.verify".into(),
            message: format!("ssh not found: {e}"),
        })?;

    if !out.status.success() {
        return Ok(None);
    }

    // sha256sum output: "<hash>  <filename>" or "<hash> *<filename>"
    let stdout = String::from_utf8_lossy(&out.stdout);
    let hash = stdout
        .split_whitespace()
        .next()
        .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
        .map(String::from);
    Ok(hash)
}

/// Compute the SHA-256 of a file, returning the result as a lowercase hex string.
fn sha256_file(path: &Path) -> Result<String, FrostxError> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().fold(String::new(), |mut acc, b| {
        use std::fmt::Write as _;
        let _ = write!(acc, "{b:02x}");
        acc
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_path_includes_uuid() {
        let b = RsyncBackend::new("rsync://server/projects");
        let uuid = uuid::Uuid::nil();
        let path = b.remote_path(uuid);
        assert!(path.starts_with("rsync://server/projects/"));
        assert!(path.ends_with(".tar.gz"));
    }

    #[test]
    fn remote_path_strips_trailing_slash() {
        let b = RsyncBackend::new("rsync://server/projects/");
        let uuid = uuid::Uuid::nil();
        let path = b.remote_path(uuid);
        assert!(!path.contains("//00000000"));
    }

    #[test]
    fn sha256_file_known_hash() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"hello world\n").unwrap();
        let hash = sha256_file(tmp.path()).unwrap();
        assert_eq!(
            hash,
            "a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447"
        );
    }

    #[test]
    fn sha256_file_empty() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let hash = sha256_file(tmp.path()).unwrap();
        // SHA-256 of empty input
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn parse_ssh_target_simple() {
        let uuid = uuid::Uuid::nil();
        let t = parse_ssh_target("ssh://user@host/backups", uuid).unwrap();
        assert_eq!(t.userhost, "user@host");
        assert_eq!(t.port, 22);
        assert!(t.remote_path.ends_with(".tar.gz"));
        assert!(t.remote_path.starts_with("/backups/"));
    }

    #[test]
    fn parse_ssh_target_with_port() {
        let uuid = uuid::Uuid::nil();
        let t = parse_ssh_target("ssh://user@host:2222/data/archives", uuid).unwrap();
        assert_eq!(t.userhost, "user@host");
        assert_eq!(t.port, 2222);
        assert!(t.remote_path.starts_with("/data/archives/"));
    }

    #[test]
    fn parse_ssh_target_no_user() {
        let uuid = uuid::Uuid::nil();
        let t = parse_ssh_target("ssh://host/path", uuid).unwrap();
        assert_eq!(t.userhost, "host");
        assert_eq!(t.port, 22);
    }

    #[test]
    fn parse_ssh_target_missing_path_returns_none() {
        let uuid = uuid::Uuid::nil();
        assert!(parse_ssh_target("ssh://host", uuid).is_none());
    }
}
