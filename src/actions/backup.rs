use super::{Action, ActionContext, ActionFactory, ActionKind, ActionOutcome};
use crate::backup;
use crate::error::FrostxError;

/// Static registration of all backup actions.
pub const REGISTRY: &[(&str, ActionFactory)] = &[
    ("backup.check", |config| Ok(Box::new(Check::new(config)?))),
    ("backup.upload", |config| Ok(Box::new(Upload::new(config)?))),
    ("backup.verify", |config| Ok(Box::new(Verify::new(config)?))),
];
use std::path::PathBuf;

fn archive_path_for(ctx: &ActionContext<'_>) -> Option<PathBuf> {
    // Look for a previously created archive from archive.compress.
    let parent = ctx.project_path.parent()?;
    let name = ctx.project_path.file_name()?.to_str()?;
    let uuid = ctx.config.id;
    // Match any archive file with this project name and uuid.
    std::fs::read_dir(parent).ok()?.find_map(|e| {
        let e = e.ok()?;
        let fname = e.file_name();
        let s = fname.to_str()?;
        if s.starts_with(name) && s.contains(&uuid.to_string()) {
            Some(e.path())
        } else {
            None
        }
    })
}

/// Check that an archive exists on the backup server.
pub struct Check {
    server: String,
}

impl Check {
    /// Construct from project config.
    ///
    /// # Errors
    ///
    /// Returns an error if `[config.backup]` is absent from the project config.
    pub fn new(config: &crate::config::project::ProjectConfig) -> Result<Self, FrostxError> {
        let server = config.require_backup()?.server.clone();
        Ok(Self { server })
    }
}

impl Action for Check {
    fn name(&self) -> &'static str {
        "backup.check"
    }
    fn kind(&self) -> ActionKind {
        ActionKind::Check
    }
    fn supports_compressed_archive(&self) -> bool {
        true
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        let backend = backup::from_url(&self.server)?;
        if backend.check(ctx.config.id)? {
            Ok(ActionOutcome::ok("archive found on backup server"))
        } else {
            Ok(ActionOutcome::failed("not found on backup server"))
        }
    }
}

/// Upload the local archive to the backup server.
pub struct Upload {
    server: String,
}

impl Upload {
    /// Construct from project config.
    ///
    /// # Errors
    ///
    /// Returns an error if `[config.backup]` is absent from the project config.
    pub fn new(config: &crate::config::project::ProjectConfig) -> Result<Self, FrostxError> {
        let server = config.require_backup()?.server.clone();
        Ok(Self { server })
    }
}

impl Action for Upload {
    fn name(&self) -> &'static str {
        "backup.upload"
    }
    fn kind(&self) -> ActionKind {
        ActionKind::Mutation
    }
    fn supports_compressed_archive(&self) -> bool {
        true
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        let archive = archive_path_for(ctx).ok_or_else(|| FrostxError::ActionFailed {
            action: "backup.upload".into(),
            message: "no local archive found - run archive.compress first".into(),
        })?;

        if ctx.dry_run {
            return Ok(ActionOutcome::dry_run(format!(
                "would upload {} to {}",
                archive.display(),
                self.server
            )));
        }

        let backend = backup::from_url(&self.server)?;
        let remote = backend.upload(ctx.config.id, &archive)?;
        Ok(ActionOutcome::ok(format!("uploaded to {remote}")))
    }
}

/// Verify that the uploaded archive is intact.
pub struct Verify {
    server: String,
}

impl Verify {
    /// Construct from project config.
    ///
    /// # Errors
    ///
    /// Returns an error if `[config.backup]` is absent from the project config.
    pub fn new(config: &crate::config::project::ProjectConfig) -> Result<Self, FrostxError> {
        let server = config.require_backup()?.server.clone();
        Ok(Self { server })
    }
}

impl Action for Verify {
    fn name(&self) -> &'static str {
        "backup.verify"
    }
    fn kind(&self) -> ActionKind {
        ActionKind::Check
    }
    fn supports_compressed_archive(&self) -> bool {
        true
    }

    fn run(&self, ctx: &ActionContext<'_>) -> Result<ActionOutcome, FrostxError> {
        let backend = backup::from_url(&self.server)?;
        if backend.verify(ctx.config.id, "")? {
            Ok(ActionOutcome::ok("backup verified"))
        } else {
            Ok(ActionOutcome::failed(
                "backup verification failed - archive not found or corrupt",
            ))
        }
    }
}
