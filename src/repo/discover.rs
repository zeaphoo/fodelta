//! Locate the repository root by walking up from a starting directory.

use std::path::{Path, PathBuf};

use crate::{FodeltaError, Result};

pub const DOT_DIR: &str = ".fodelta";

/// Find the nearest ancestor (including `start`) that contains `.fodelta/`.
pub fn discover(start: &Path) -> Result<PathBuf> {
    let canonical = start.canonicalize()?;
    let mut current = if canonical.is_dir() {
        canonical
    } else {
        canonical
            .parent()
            .ok_or_else(|| FodeltaError::NotARepository {
                start: start.to_path_buf(),
            })?
            .to_path_buf()
    };

    loop {
        if current.join(DOT_DIR).is_dir() {
            return Ok(current);
        }
        if !current.pop() {
            break;
        }
    }
    Err(FodeltaError::NotARepository {
        start: start.to_path_buf(),
    })
}

/// Check whether `start` is inside an existing repository (excluding `start`
/// itself). Used by `init` to refuse nested repositories.
pub fn find_ancestor_repository(start: &Path) -> Option<PathBuf> {
    let mut current = start.parent();
    while let Some(directory) = current {
        if directory.join(DOT_DIR).is_dir() {
            return Some(directory.to_path_buf());
        }
        current = directory.parent();
    }
    None
}
