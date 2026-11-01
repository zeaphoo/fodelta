//! Repository-relative path helpers.
//!
//! Paths are stored as `/`-separated bytes. Unix paths map to raw bytes;
//! Windows paths are converted lossily because the tree format is byte based.

use std::path::{Component, Path, PathBuf};

use crate::{FodeltaError, Result};

/// Convert a relative path into `/`-separated bytes.
pub fn path_to_bytes(path: &Path) -> Vec<u8> {
    let mut out = Vec::new();
    for component in path.components() {
        if let Component::Normal(name) = component {
            if !out.is_empty() {
                out.push(b'/');
            }
            out.extend_from_slice(name.as_encoded_bytes());
        }
    }
    out
}

/// Convert `/`-separated bytes back into a platform path.
pub fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    if bytes.is_empty() {
        return PathBuf::new();
    }
    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let mut path = PathBuf::new();
        for component in bytes.split(|byte| *byte == b'/') {
            path.push(OsStr::from_bytes(component));
        }
        path
    }
    #[cfg(not(unix))]
    {
        let text = String::from_utf8_lossy(bytes);
        let mut path = PathBuf::new();
        for component in text.split('/') {
            path.push(component);
        }
        path
    }
}

/// Lexically normalize a user supplied path, rejecting absolute paths and
/// parent traversal outside the repository.
pub fn normalize(input: &Path) -> Result<PathBuf> {
    if input.is_absolute() {
        return Err(FodeltaError::PathEscapes {
            path: input.to_path_buf(),
        });
    }
    let mut out = PathBuf::new();
    for component in input.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => out.push(name),
            Component::ParentDir => {
                if !out.pop() {
                    return Err(FodeltaError::PathEscapes {
                        path: input.to_path_buf(),
                    });
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(FodeltaError::PathEscapes {
                    path: input.to_path_buf(),
                });
            }
        }
    }
    Ok(out)
}

pub fn join(root: &Path, bytes: &[u8]) -> PathBuf {
    root.join(bytes_to_path(bytes))
}

/// True when `path` equals `prefix` or lives below it. An empty prefix matches
/// everything.
pub fn is_within(prefix: &[u8], path: &[u8]) -> bool {
    if prefix.is_empty() {
        return true;
    }
    if path == prefix {
        return true;
    }
    path.len() > prefix.len() && path.starts_with(prefix) && path.get(prefix.len()) == Some(&b'/')
}

pub fn display(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn parent(bytes: &[u8]) -> Option<&[u8]> {
    bytes
        .iter()
        .rposition(|byte| *byte == b'/')
        .map(|index| &bytes[..index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_roundtrip() {
        let path = Path::new("a/b/c.txt");
        assert_eq!(path_to_bytes(path), b"a/b/c.txt");
        assert_eq!(bytes_to_path(b"a/b/c.txt"), path);
        assert_eq!(bytes_to_path(b""), PathBuf::new());
    }

    #[test]
    fn normalize_rejects_escapes() {
        assert!(normalize(Path::new("../x")).is_err());
        assert!(normalize(Path::new("/x")).is_err());
        assert_eq!(
            normalize(Path::new("a/./b/../c")).unwrap(),
            Path::new("a/c")
        );
        assert_eq!(normalize(Path::new(".")).unwrap(), PathBuf::new());
    }

    #[test]
    fn prefix_matching_is_component_wise() {
        assert!(is_within(b"src", b"src/a.rs"));
        assert!(is_within(b"src", b"src"));
        assert!(!is_within(b"src", b"src2/a.rs"));
        assert!(is_within(b"", b"anything"));
    }
}
