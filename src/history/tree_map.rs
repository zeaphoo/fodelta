//! Flatten and rebuild trees as `path -> (mode, oid)` maps.

use std::collections::BTreeMap;

use crate::odb::{Commit, MODE_TREE, ObjectId, ObjectKind, ObjectStore, Tree, TreeEntry};
use crate::repo::Repository;
use crate::{FodeltaError, Result};

const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeEntryInfo {
    pub mode: u32,
    pub oid: ObjectId,
}

pub type TreeMap = BTreeMap<Vec<u8>, TreeEntryInfo>;

/// Expand a tree object into a flat map of file paths.
pub fn flatten_tree(store: &ObjectStore, root: ObjectId) -> Result<TreeMap> {
    let mut out = TreeMap::new();
    flatten_into(store, root, &mut Vec::new(), &mut out, 0)?;
    Ok(out)
}

fn flatten_into(
    store: &ObjectStore,
    oid: ObjectId,
    prefix: &mut Vec<u8>,
    out: &mut TreeMap,
    depth: usize,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(FodeltaError::RepositoryCorrupt(
            "tree nesting is too deep".to_string(),
        ));
    }
    let (kind, payload) = store.read(oid)?;
    if kind != ObjectKind::Tree {
        return Err(FodeltaError::RepositoryCorrupt(format!(
            "object {} is not a tree",
            oid.short()
        )));
    }
    let tree = Tree::decode(&payload)?;
    for entry in tree.entries() {
        let prefix_len = prefix.len();
        if !prefix.is_empty() {
            prefix.push(b'/');
        }
        prefix.extend_from_slice(&entry.name);
        if entry.is_dir() {
            flatten_into(store, entry.oid, prefix, out, depth + 1)?;
        } else {
            out.insert(
                prefix.clone(),
                TreeEntryInfo {
                    mode: entry.mode,
                    oid: entry.oid,
                },
            );
        }
        prefix.truncate(prefix_len);
    }
    Ok(())
}

enum Node {
    Leaf(TreeEntryInfo),
    Dir(BTreeMap<Vec<u8>, Node>),
}

/// Build all tree objects for a flat map and return the root tree id.
pub fn build_trees(store: &ObjectStore, entries: &TreeMap) -> Result<ObjectId> {
    let mut root: BTreeMap<Vec<u8>, Node> = BTreeMap::new();
    for (path, info) in entries {
        let components: Vec<&[u8]> = path.split(|byte| *byte == b'/').collect();
        if components.is_empty() || components.iter().any(|component| component.is_empty()) {
            return Err(FodeltaError::InvalidObject {
                reason: format!("invalid path '{}'", crate::util::path::display(path)),
            });
        }
        insert_node(&mut root, &components, *info)?;
    }
    let (_, oid) = write_node(store, &Node::Dir(root))?;
    Ok(oid)
}

fn insert_node(
    children: &mut BTreeMap<Vec<u8>, Node>,
    components: &[&[u8]],
    info: TreeEntryInfo,
) -> Result<()> {
    let (name, rest) = components
        .split_first()
        .ok_or_else(|| FodeltaError::InvalidObject {
            reason: "empty tree path".to_string(),
        })?;
    if rest.is_empty() {
        if children.contains_key(*name) {
            return Err(FodeltaError::InvalidObject {
                reason: format!("path conflict at '{}'", crate::util::path::display(name)),
            });
        }
        children.insert(name.to_vec(), Node::Leaf(info));
        return Ok(());
    }

    let node = children
        .entry(name.to_vec())
        .or_insert_with(|| Node::Dir(BTreeMap::new()));
    match node {
        Node::Dir(children) => insert_node(children, rest, info),
        Node::Leaf(_) => Err(FodeltaError::InvalidObject {
            reason: format!(
                "path conflict: '{}' is both a file and a directory",
                crate::util::path::display(name)
            ),
        }),
    }
}

fn write_node(store: &ObjectStore, node: &Node) -> Result<(u32, ObjectId)> {
    match node {
        Node::Leaf(info) => Ok((info.mode, info.oid)),
        Node::Dir(children) => {
            let mut entries = Vec::with_capacity(children.len());
            for (name, child) in children {
                let (mode, oid) = write_node(store, child)?;
                entries.push(TreeEntry::new(mode, oid, name.clone()));
            }
            let tree = Tree::new(entries)?;
            let payload = tree.encode()?;
            let oid = store.write(ObjectKind::Tree, &payload)?;
            Ok((MODE_TREE, oid))
        }
    }
}

/// Load the tree of the current HEAD, or an empty map when HEAD is unborn.
pub fn head_tree(repo: &Repository) -> Result<TreeMap> {
    let Some(oid) = repo.head_oid()? else {
        return Ok(TreeMap::new());
    };
    let store = repo.store();
    let (kind, payload) = store.read(oid)?;
    if kind != ObjectKind::Commit {
        return Err(FodeltaError::RepositoryCorrupt(format!(
            "HEAD points to non-commit object {}",
            oid.short()
        )));
    }
    let commit = Commit::decode(&payload)?;
    flatten_tree(&store, commit.tree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::odb::ObjectStore;
    use crate::odb::{MODE_EXEC, MODE_FILE, ObjectId};

    fn store(dir: &std::path::Path) -> ObjectStore {
        ObjectStore::new(dir.join("objects"), 3, 1024 * 1024, false)
    }

    fn info(mode: u32, byte: u8) -> TreeEntryInfo {
        TreeEntryInfo {
            mode,
            oid: ObjectId::from_bytes([byte; 32]),
        }
    }

    #[test]
    fn build_and_flatten_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let mut entries = TreeMap::new();
        entries.insert(b"a.txt".to_vec(), info(MODE_FILE, 1));
        entries.insert(b"src/b.rs".to_vec(), info(MODE_FILE, 2));
        entries.insert(b"src/run.sh".to_vec(), info(MODE_EXEC, 3));

        let root = build_trees(&store, &entries).unwrap();
        assert_eq!(flatten_tree(&store, root).unwrap(), entries);
    }

    #[test]
    fn rejects_file_directory_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let mut entries = TreeMap::new();
        entries.insert(b"a".to_vec(), info(MODE_FILE, 1));
        entries.insert(b"a/b".to_vec(), info(MODE_FILE, 2));
        assert!(build_trees(&store, &entries).is_err());
    }

    #[test]
    fn empty_tree_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let root = build_trees(&store, &TreeMap::new()).unwrap();
        assert!(flatten_tree(&store, root).unwrap().is_empty());
    }
}
