//! Repository layout, configuration and reference handling.

pub mod config;
pub mod discover;
pub mod init;
pub mod lock;
pub mod refs;

use std::path::{Path, PathBuf};

use crate::odb::{Author, ObjectId, ObjectStore};
use crate::{FodeltaError, Result};
use config::Config;
use discover::DOT_DIR;
use lock::RepoLock;

pub use config::{CoreConfig, DiffConfig, FsyncPolicy, UserConfig};
pub use init::{detect_user, init};
pub use refs::MAIN_REF;

#[derive(Debug)]
pub struct Repository {
    pub root: PathBuf,
    pub fodelta: PathBuf,
    pub config: Config,
}

impl Repository {
    pub fn open(start: &Path) -> Result<Self> {
        let root = discover::discover(start)?;
        let fodelta = root.join(DOT_DIR);
        let config = Config::load(&fodelta.join("config.toml"))?;
        Ok(Self {
            root,
            fodelta,
            config,
        })
    }

    pub fn init(target: &Path) -> Result<Self> {
        let root = init::init(target)?;
        Self::open(&root)
    }

    pub fn config_path(&self) -> PathBuf {
        self.fodelta.join("config.toml")
    }

    pub fn store(&self) -> ObjectStore {
        ObjectStore::new(
            self.fodelta.join("objects"),
            self.config.core.compression,
            self.config.max_object_size_bytes(),
            self.config.core.fsync.fsync_objects(),
        )
    }

    pub fn lock(&self) -> Result<RepoLock> {
        RepoLock::acquire(&self.fodelta)
    }

    pub fn head_oid(&self) -> Result<Option<ObjectId>> {
        refs::head_oid(&self.fodelta)
    }

    pub fn require_head(&self) -> Result<ObjectId> {
        self.head_oid()?.ok_or(FodeltaError::UnbornHead)
    }

    pub fn author(&self) -> Result<Author> {
        let name = std::env::var("FODELTA_AUTHOR_NAME")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.config.user.name.clone());
        let email = std::env::var("FODELTA_AUTHOR_EMAIL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.config.user.email.clone());

        if name.trim().is_empty() {
            return Err(FodeltaError::Config(
                "user.name is not set; run 'fodelta config user.name \"Your Name\"'".to_string(),
            ));
        }
        if email.trim().is_empty() {
            return Err(FodeltaError::Config(
                "user.email is not set; run 'fodelta config user.email \"you@example.com\"'"
                    .to_string(),
            ));
        }
        Author::now(name, email)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_finds_root_from_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let nested = repo.root.join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        let reopened = Repository::open(&nested).unwrap();
        assert_eq!(reopened.root, repo.root);
    }

    #[test]
    fn configured_author_is_used() {
        if std::env::var_os("FODELTA_AUTHOR_NAME").is_some()
            || std::env::var_os("FODELTA_AUTHOR_EMAIL").is_some()
        {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repository::init(dir.path()).unwrap();
        repo.config.user.name = "Ada".to_string();
        repo.config.user.email = "ada@example.com".to_string();
        let author = repo.author().unwrap();
        assert_eq!(author.name, "Ada");
        assert_eq!(author.email, "ada@example.com");
    }

    #[test]
    fn empty_identity_is_rejected() {
        if std::env::var_os("FODELTA_AUTHOR_EMAIL").is_some() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repository::init(dir.path()).unwrap();
        repo.config.user.email.clear();
        let error = repo.author().unwrap_err();
        assert!(matches!(error, FodeltaError::Config(_)));
    }

    #[test]
    fn open_outside_repository_fails() {
        let dir = tempfile::tempdir().unwrap();
        let error = Repository::open(dir.path()).unwrap_err();
        assert!(matches!(error, FodeltaError::NotARepository { .. }));
    }
}
