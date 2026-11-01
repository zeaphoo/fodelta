//! Inspect a commit or a blob inside a revision.

use crate::history::revparse::{read_commit, resolve};
use crate::odb::{Commit, ObjectId};
use crate::repo::Repository;
use crate::util::path;
use crate::{FodeltaError, Result};

#[derive(Debug, Clone)]
pub struct CommitView {
    pub oid: ObjectId,
    pub commit: Commit,
}

#[derive(Debug, Clone)]
pub enum ShowOutput {
    Commit(CommitView),
    Blob(Vec<u8>),
}

pub fn show(repo: &Repository, spec: &str) -> Result<ShowOutput> {
    let (rev, file) = match spec.split_once(':') {
        Some((rev, file)) => (rev, Some(file)),
        None => (spec, None),
    };
    let oid = resolve(repo, rev)?;
    let commit = read_commit(repo, oid)?;

    if let Some(file) = file {
        if file.is_empty() {
            return Err(FodeltaError::PathMissingInRev {
                rev: rev.to_string(),
                path: std::path::PathBuf::new(),
            });
        }
        let relative = path::path_to_bytes(&path::normalize(std::path::Path::new(file))?);
        let store = repo.store();
        let tree = crate::history::tree_map::flatten_tree(&store, commit.tree)?;
        if !tree.contains_key(&relative) {
            return Err(FodeltaError::PathMissingInRev {
                rev: rev.to_string(),
                path: std::path::PathBuf::from(file),
            });
        }
        let info = tree.get(&relative).expect("checked above");
        return Ok(ShowOutput::Blob(store.read_blob(info.oid)?));
    }

    Ok(ShowOutput::Commit(CommitView { oid, commit }))
}
