//! `frostx doctor` - validate `frostx.toml` without running anything.

use crate::config;
use crate::error::FrostxError;
use crate::output::{DoctorItem, DoctorOutput, FROSTX_VERSION};
use std::path::Path;

use super::FrostxOpts;

/// Validate `frostx.toml` for the project at `path`.
///
/// Returns a [`DoctorOutput`] describing any errors or warnings found.
/// Does not execute any actions or modify any state.
pub fn execute(path: &Path, opts: &FrostxOpts) -> Result<DoctorOutput, FrostxError> {
    let cfg = super::init::load_config(path, opts)?;
    let result = config::validate(&cfg);

    Ok(DoctorOutput {
        frostx_version: FROSTX_VERSION,
        valid: result.errors.is_empty(),
        errors: result
            .errors
            .iter()
            .map(|m| DoctorItem {
                field: "-".into(),
                message: m.clone(),
            })
            .collect(),
        warnings: result
            .warnings
            .iter()
            .map(|m| DoctorItem {
                field: "-".into(),
                message: m.clone(),
            })
            .collect(),
    })
}
