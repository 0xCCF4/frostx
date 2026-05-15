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

/// Factory function type: creates a [`Box<dyn BackupBackend>`] from a server URL string.
pub type BackendFactory = fn(&str) -> Box<dyn BackupBackend>;

/// rsync/ssh backup backend.
pub mod rsync;

/// All per-module static backend registries.
///
/// To add a new backend: create a submodule, implement [`BackupBackend`], expose a
/// `pub const REGISTRY: &[(&str, BackendFactory)]` mapping URL scheme prefixes to
/// constructors, then add the module's `REGISTRY` to this list. No other changes
/// are needed.
const ALL_BACKENDS: &[&[(&str, BackendFactory)]] = &[rsync::REGISTRY];

/// Parse a server URL and return the appropriate backend.
///
/// Iterates [`ALL_BACKENDS`] and returns the first backend whose scheme prefix
/// matches. Adding support for a new URL scheme only requires a new entry in the
/// relevant module's `REGISTRY`.
///
/// # Errors
///
/// Returns an error if no registered backend handles the URL scheme.
pub fn from_url(server: &str) -> Result<Box<dyn BackupBackend>, FrostxError> {
    for registry in ALL_BACKENDS {
        for (scheme, factory) in *registry {
            if server.starts_with(scheme) {
                return Ok(factory(server));
            }
        }
    }
    let supported: Vec<&str> = ALL_BACKENDS
        .iter()
        .flat_map(|r| r.iter().map(|(s, _)| *s))
        .collect();
    Err(FrostxError::Config(format!(
        "unsupported backup server scheme in '{server}': expected one of {}",
        supported.join(", ")
    )))
}
