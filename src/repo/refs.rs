//! `HEAD` and reference files.

use std::fs;
use std::path::{Path, PathBuf};

use crate::odb::ObjectId;
use crate::util::atomic::write_atomic;
use crate::{FodeltaError, Result};

pub const HEAD_FILE: &str = "HEAD";
pub const MAIN_REF: &str = "refs/heads/main";

/// Read `HEAD` and return the reference it points at.
pub fn head_ref(fodelta: &Path) -> Result<String> {
    let path = fodelta.join(HEAD_FILE);
    let text = fs::read_to_string(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            FodeltaError::RepositoryCorrupt("HEAD is missing".to_string())
        } else {
            FodeltaError::Io(error)
        }
    })?;
    let reference = text
        .strip_prefix("ref: ")
        .ok_or_else(|| FodeltaError::RepositoryCorrupt("HEAD is not a symbolic reference".into()))?
        .trim();
    validate_ref_name(reference)?;
    Ok(reference.to_string())
}

pub fn read_ref(fodelta: &Path, reference: &str) -> Result<Option<ObjectId>> {
    validate_ref_name(reference)?;
    let path = ref_path(fodelta, reference);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let trimmed = text.trim();
    let oid = ObjectId::from_hex(trimmed).map_err(|_| {
        FodeltaError::RepositoryCorrupt(format!(
            "reference {reference} does not contain a valid object id"
        ))
    })?;
    Ok(Some(oid))
}

pub fn head_oid(fodelta: &Path) -> Result<Option<ObjectId>> {
    let reference = head_ref(fodelta)?;
    read_ref(fodelta, &reference)
}

/// Write a reference without checking its previous value. Callers that mutate
/// history must use [`update_ref`] under the repository lock.
pub fn write_ref(fodelta: &Path, reference: &str, oid: ObjectId, fsync: bool) -> Result<()> {
    validate_ref_name(reference)?;
    let path = ref_path(fodelta, reference);
    let contents = format!("{oid}\n");
    write_atomic(&path, contents.as_bytes(), fsync)?;
    Ok(())
}

/// Compare-and-swap a reference: the current value must equal `expected`.
pub fn update_ref(
    fodelta: &Path,
    reference: &str,
    expected: Option<ObjectId>,
    new: ObjectId,
    fsync: bool,
) -> Result<()> {
    let current = read_ref(fodelta, reference)?;
    if current != expected {
        return Err(FodeltaError::RefConflict {
            reference: reference.to_string(),
            expected: expected.map(|oid| oid.to_hex()),
            actual: current.map(|oid| oid.to_hex()),
        });
    }
    write_ref(fodelta, reference, new, fsync)
}

pub fn ref_path(fodelta: &Path, reference: &str) -> PathBuf {
    fodelta.join(reference)
}

fn validate_ref_name(reference: &str) -> Result<()> {
    if reference.is_empty() || reference.starts_with('/') || reference.ends_with('/') {
        return Err(FodeltaError::RepositoryCorrupt(format!(
            "invalid reference name '{reference}'"
        )));
    }
    for component in reference.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(FodeltaError::RepositoryCorrupt(format!(
                "invalid reference name '{reference}'"
            )));
        }
        if component.contains(['\0', '\\']) {
            return Err(FodeltaError::RepositoryCorrupt(format!(
                "invalid reference name '{reference}'"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".fodelta")).unwrap();
        fs::write(dir.path().join(".fodelta/HEAD"), "ref: refs/heads/main\n").unwrap();
        dir
    }

    #[test]
    fn unborn_head_is_none() {
        let dir = repo();
        let fodelta = dir.path().join(".fodelta");
        assert_eq!(head_ref(&fodelta).unwrap(), MAIN_REF);
        assert_eq!(head_oid(&fodelta).unwrap(), None);
    }

    #[test]
    fn write_read_and_cas() {
        let dir = repo();
        let fodelta = dir.path().join(".fodelta");
        let first = ObjectId::from_bytes([1; 32]);
        let second = ObjectId::from_bytes([2; 32]);

        update_ref(&fodelta, MAIN_REF, None, first, false).unwrap();
        assert_eq!(head_oid(&fodelta).unwrap(), Some(first));

        let err = update_ref(&fodelta, MAIN_REF, None, second, false).unwrap_err();
        assert!(matches!(err, FodeltaError::RefConflict { .. }));

        update_ref(&fodelta, MAIN_REF, Some(first), second, false).unwrap();
        assert_eq!(head_oid(&fodelta).unwrap(), Some(second));
    }

    #[test]
    fn malformed_head_is_reported() {
        let dir = repo();
        let head = dir.path().join(".fodelta/HEAD");
        fs::write(&head, "main\n").unwrap();
        let err = head_ref(dir.path().join(".fodelta").as_path()).unwrap_err();
        assert!(matches!(err, FodeltaError::RepositoryCorrupt(_)));
    }
}
