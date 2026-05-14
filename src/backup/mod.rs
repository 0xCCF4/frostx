use crate::error::FrostxError;
use std::path::Path;
use uuid::Uuid;

/// Trait for backup backends. Implement this to add new storage targets.
pub trait BackupBackend: Send + Sync {
    /// Check if an archive for `uuid` exists on the backend.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend cannot be contacted or the check fails.
    fn check(&self, uuid: Uuid) -> Result<bool, FrostxError>;

    /// Upload `archive_path` to the backend, keyed by `uuid`.
    ///
    /// # Errors
    ///
    /// Returns an error if the upload fails or the backend is unreachable.
    fn upload(&self, uuid: Uuid, archive_path: &Path) -> Result<String, FrostxError>;

    /// Verify that the archive on the backend matches `expected_checksum`.
    ///
    /// # Errors
    ///
    /// Returns an error if verification cannot be performed.
    fn verify(&self, uuid: Uuid, expected_checksum: &str) -> Result<bool, FrostxError>;
}

/// rsync/ssh backup backend.
pub mod rsync;

/// Parse a server URL and return the appropriate backend.
///
/// # Errors
///
/// Returns an error if the URL scheme is not `rsync://` or `ssh://`.
pub fn from_url(server: &str) -> Result<Box<dyn BackupBackend>, FrostxError> {
    if server.starts_with("rsync://") || server.starts_with("ssh://") {
        Ok(Box::new(rsync::RsyncBackend::new(server)))
    } else {
        Err(FrostxError::Config(format!(
            "unsupported backup server scheme in '{server}': expected rsync:// or ssh://"
        )))
    }
}
