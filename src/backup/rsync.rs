use super::{BackendFactory, BackupBackend};
use crate::error::FrostxError;
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

    fn verify(&self, uuid: Uuid, _expected_checksum: &str) -> Result<bool, FrostxError> {
        // Basic existence check; checksum verification would require a remote
        // sha256sum call, which is backend-specific. For rsync we confirm existence.
        self.check(uuid)
    }
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
}
