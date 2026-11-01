//! `.gitignore` diagnostics.
//!
//! Scanning itself is delegated to the `ignore` crate; this module explains
//! which `.gitignore` file and pattern would decide a path.

use std::fs;
use std::path::{Path, PathBuf};

use ignore::Match;
use ignore::gitignore::GitignoreBuilder;

use crate::util::path;
use crate::worktree::scan;
use crate::{FodeltaError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreVerdict {
    pub ignored: bool,
    pub builtin: bool,
    pub source: Option<PathBuf>,
    pub pattern: Option<String>,
}

pub fn check(root: &Path, relative: &[u8]) -> Result<IgnoreVerdict> {
    if relative.is_empty() {
        return Ok(not_ignored());
    }

    if let Some(reason) = builtin_reason(root, relative) {
        return Ok(IgnoreVerdict {
            ignored: true,
            builtin: true,
            source: None,
            pattern: Some(reason),
        });
    }

    let absolute = path::join(root, relative);
    let is_dir = absolute.is_dir();
    let parent = path::parent(relative).unwrap_or_default();
    let mut directories = Vec::new();
    let mut current = PathBuf::new();
    directories.push(current.clone());
    if !parent.is_empty() {
        for component in path::bytes_to_path(parent).components() {
            current.push(component);
            directories.push(current.clone());
        }
    }

    // Deeper .gitignore files win, so walk from the path's directory upward.
    for directory in directories.iter().rev() {
        let ignore_file = root.join(directory).join(".gitignore");
        if !ignore_file.is_file() {
            continue;
        }
        let base = path::path_to_bytes(directory);
        if !path::is_within(&base, relative) {
            continue;
        }
        let relative_to_base = if base.is_empty() {
            relative
        } else if relative.len() == base.len() {
            b""
        } else {
            &relative[base.len() + 1..]
        };
        if relative_to_base.is_empty() {
            continue;
        }
        let mut builder = GitignoreBuilder::new(root.join(directory));
        if let Some(error) = builder.add(&ignore_file) {
            return Err(FodeltaError::Config(format!(
                "cannot parse {}: {error}",
                ignore_file.display()
            )));
        }
        let matcher = builder.build().map_err(|error| {
            FodeltaError::Config(format!("cannot parse {}: {error}", ignore_file.display()))
        })?;
        match matcher.matched_path_or_any_parents(path::bytes_to_path(relative_to_base), is_dir) {
            Match::Ignore(glob) => {
                return Ok(IgnoreVerdict {
                    ignored: true,
                    builtin: false,
                    source: Some(relative_path(root, &ignore_file)),
                    pattern: Some(glob.original().to_string()),
                });
            }
            Match::Whitelist(glob) => {
                return Ok(IgnoreVerdict {
                    ignored: false,
                    builtin: false,
                    source: Some(relative_path(root, &ignore_file)),
                    pattern: Some(glob.original().to_string()),
                });
            }
            Match::None => {}
        }
    }

    Ok(not_ignored())
}

fn not_ignored() -> IgnoreVerdict {
    IgnoreVerdict {
        ignored: false,
        builtin: false,
        source: None,
        pattern: None,
    }
}

fn builtin_reason(root: &Path, relative: &[u8]) -> Option<String> {
    let first = relative.split(|byte| *byte == b'/').next()?;
    if first == b".fodelta" || first == b".git" {
        return Some("built-in repository directory".to_string());
    }
    // A nested repository is skipped entirely.
    let mut current = root.to_path_buf();
    let components: Vec<&[u8]> = relative.split(|byte| *byte == b'/').collect();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        current = current.join(path::bytes_to_path(component));
        if current.join(".fodelta").is_dir() {
            return Some("nested fodelta repository".to_string());
        }
    }
    None
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

/// All `.gitignore` files that participate in scanning, sorted.
pub fn files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = scan::scan_all(root)?
        .into_iter()
        .filter(|entry| {
            entry
                .path
                .rsplit(|byte| *byte == b'/')
                .next()
                .is_some_and(|name| name == b".gitignore")
        })
        .map(|entry| path::bytes_to_path(&entry.path))
        .collect();
    files.sort();
    Ok(files)
}

pub fn read_rules(file: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(file)?;
    Ok(text
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, name: &str, contents: &[u8]) {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn finds_nested_rule_sources() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".gitignore", b"*.log\n");
        write(dir.path(), "src/.gitignore", b"!keep.log\n");
        write(dir.path(), "src/keep.log", b"x");
        write(dir.path(), "other.log", b"x");

        let verdict = check(dir.path(), b"src/keep.log").unwrap();
        assert!(!verdict.ignored);
        assert!(verdict.source.unwrap().ends_with("src/.gitignore"));

        let verdict = check(dir.path(), b"other.log").unwrap();
        assert!(verdict.ignored);
        assert_eq!(verdict.pattern.as_deref(), Some("*.log"));
    }

    #[test]
    fn builtin_directories_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".fodelta")).unwrap();
        let verdict = check(dir.path(), b".fodelta/HEAD").unwrap();
        assert!(verdict.ignored);
        assert!(verdict.builtin);
    }

    #[test]
    fn lists_gitignore_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".gitignore", b"*.log\n");
        write(dir.path(), "src/.gitignore", b"*.tmp\n");
        write(dir.path(), "src/main.rs", b"x");
        let files = files(dir.path()).unwrap();
        assert_eq!(
            files,
            vec![PathBuf::from(".gitignore"), PathBuf::from("src/.gitignore")]
        );
    }
}
