//! `fodelta init` implementation.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::repo::config::Config;
use crate::repo::discover::{DOT_DIR, find_ancestor_repository};
use crate::util::atomic::write_atomic;
use crate::{FodeltaError, Result};

pub fn init(target: &Path) -> Result<PathBuf> {
    if target.join(DOT_DIR).exists() {
        return Err(FodeltaError::RepositoryExists {
            root: target.to_path_buf(),
        });
    }

    fs::create_dir_all(target)?;
    let root = target.canonicalize()?;
    if root.join(DOT_DIR).exists() {
        return Err(FodeltaError::RepositoryExists { root });
    }
    if let Some(ancestor) = find_ancestor_repository(&root) {
        return Err(FodeltaError::NestedRepository { root, ancestor });
    }

    let fodelta = root.join(DOT_DIR);
    fs::create_dir_all(fodelta.join("objects"))?;
    fs::create_dir_all(fodelta.join("refs/heads"))?;
    fs::create_dir_all(fodelta.join("logs"))?;
    fs::write(fodelta.join("logs/safety"), b"")?;
    write_atomic(&fodelta.join("HEAD"), b"ref: refs/heads/main\n", true)?;

    let (name, email) = detect_user();
    Config::for_new_repo(name, email).save(&fodelta.join("config.toml"))?;

    Ok(root)
}

/// Derive a default identity from the environment. The user can edit it with
/// `fodelta config` afterwards.
pub fn detect_user() -> (String, String) {
    let name = env::var("FODELTA_AUTHOR_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env::var("USER").ok().filter(|value| !value.is_empty()))
        .or_else(|| env::var("USERNAME").ok().filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "fodelta".to_string());

    let host = env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            env::var("COMPUTERNAME")
                .ok()
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "localhost".to_string());

    let email = env::var("FODELTA_AUTHOR_EMAIL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{}@{}", sanitize(&name, "._%+-"), sanitize(&host, ".-")));

    (name, email)
}

fn sanitize(value: &str, allowed_punctuation: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || allowed_punctuation.contains(character) {
                character
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.trim_matches('-').is_empty() {
        "fodelta".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_expected_layout() {
        let dir = tempfile::tempdir().unwrap();
        let root = init(dir.path()).unwrap();
        let fodelta = root.join(DOT_DIR);
        assert!(fodelta.join("HEAD").is_file());
        assert!(fodelta.join("config.toml").is_file());
        assert!(fodelta.join("objects").is_dir());
        assert!(fodelta.join("refs/heads").is_dir());
        assert!(fodelta.join("logs/safety").is_file());
        assert!(!fodelta.join("info/exclude").exists());
        assert_eq!(
            fs::read_to_string(fodelta.join("HEAD")).unwrap(),
            "ref: refs/heads/main\n"
        );
        let config = Config::load(&fodelta.join("config.toml")).unwrap();
        assert_eq!(config.format_version, 1);
        assert!(!config.user.email.is_empty());
    }

    #[test]
    fn refuses_repository_inside_repository() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path()).unwrap();
        let nested = dir.path().join("nested");
        fs::create_dir_all(&nested).unwrap();
        let error = init(&nested).unwrap_err();
        assert!(matches!(error, FodeltaError::NestedRepository { .. }));
    }

    #[test]
    fn refuses_double_init() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path()).unwrap();
        let error = init(dir.path()).unwrap_err();
        assert!(matches!(error, FodeltaError::RepositoryExists { .. }));
    }
}
