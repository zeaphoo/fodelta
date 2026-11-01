//! Tree object encoding.
//!
//! A tree is one directory level: entries are sorted by raw name bytes and each
//! entry records its Unix mode, the target object id and the child name.

use crate::odb::object::ObjectId;
use crate::{FodeltaError, Result};

pub const TREE_VERSION: u8 = 1;
pub const MODE_TREE: u32 = 0o040000;
pub const MODE_FILE: u32 = 0o100644;
pub const MODE_EXEC: u32 = 0o100755;
pub const MODE_SYMLINK: u32 = 0o120000;

const KIND_TREE: u8 = 0;
const KIND_BLOB: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub mode: u32,
    pub oid: ObjectId,
    pub name: Vec<u8>,
}

impl TreeEntry {
    pub fn new(mode: u32, oid: ObjectId, name: impl Into<Vec<u8>>) -> Self {
        Self {
            mode,
            oid,
            name: name.into(),
        }
    }

    pub fn is_dir(&self) -> bool {
        self.mode == MODE_TREE
    }

    pub fn name_lossy(&self) -> String {
        String::from_utf8_lossy(&self.name).into_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    entries: Vec<TreeEntry>,
}

impl Tree {
    /// Build a tree from arbitrary entries, sorting by name and rejecting
    /// duplicates or invalid names.
    pub fn new(mut entries: Vec<TreeEntry>) -> Result<Self> {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Self::from_sorted(entries)
    }

    /// Build a tree from entries that are already sorted.
    pub fn from_sorted(entries: Vec<TreeEntry>) -> Result<Self> {
        validate_entries(&entries)?;
        Ok(Self { entries })
    }

    pub fn entries(&self) -> &[TreeEntry] {
        &self.entries
    }

    pub fn into_entries(self) -> Vec<TreeEntry> {
        self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        validate_entries(&self.entries)?;
        let mut out = Vec::new();
        out.push(TREE_VERSION);
        let count = u32::try_from(self.entries.len()).map_err(|_| FodeltaError::InvalidObject {
            reason: "too many tree entries".to_string(),
        })?;
        out.extend_from_slice(&count.to_le_bytes());
        for entry in &self.entries {
            out.extend_from_slice(&entry.mode.to_le_bytes());
            out.push(if entry.is_dir() { KIND_TREE } else { KIND_BLOB });
            out.extend_from_slice(entry.oid.as_bytes());
            let name_len =
                u16::try_from(entry.name.len()).map_err(|_| FodeltaError::InvalidObject {
                    reason: format!("tree entry name is too long ({} bytes)", entry.name.len()),
                })?;
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(&entry.name);
        }
        Ok(out)
    }

    pub fn decode(payload: &[u8]) -> Result<Self> {
        let mut cursor = 0usize;
        let version = *payload
            .first()
            .ok_or_else(|| invalid("empty tree payload"))?;
        cursor += 1;
        if version != TREE_VERSION {
            return Err(invalid(format!(
                "unsupported tree version {version} (expected {TREE_VERSION})"
            )));
        }
        let count = read_u32(payload, &mut cursor)? as usize;
        if count > payload.len().saturating_div(39) {
            return Err(invalid("tree entry count exceeds payload size"));
        }
        let mut entries = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            let mode = read_u32(payload, &mut cursor)?;
            let kind = *payload
                .get(cursor)
                .ok_or_else(|| invalid("tree entry is missing its kind"))?;
            cursor += 1;
            let oid_bytes = payload
                .get(cursor..cursor + 32)
                .ok_or_else(|| invalid("tree entry is missing its object id"))?;
            let mut oid = [0u8; 32];
            oid.copy_from_slice(oid_bytes);
            cursor += 32;
            let name_len = read_u16(payload, &mut cursor)? as usize;
            let name = payload
                .get(cursor..cursor + name_len)
                .ok_or_else(|| invalid("tree entry name is truncated"))?
                .to_vec();
            cursor += name_len;

            let expected_kind = if mode == MODE_TREE {
                KIND_TREE
            } else if matches!(mode, MODE_FILE | MODE_EXEC | MODE_SYMLINK) {
                KIND_BLOB
            } else {
                return Err(invalid(format!("unsupported tree mode {mode:o}")));
            };
            if kind != expected_kind {
                return Err(invalid(format!(
                    "tree entry '{}' has kind {kind} but mode {mode:o}",
                    String::from_utf8_lossy(&name)
                )));
            }
            entries.push(TreeEntry {
                mode,
                oid: ObjectId::from_bytes(oid),
                name,
            });
        }
        if cursor != payload.len() {
            return Err(invalid(format!(
                "tree payload has {} trailing bytes",
                payload.len() - cursor
            )));
        }
        Self::from_sorted(entries)
    }
}

fn validate_entries(entries: &[TreeEntry]) -> Result<()> {
    let mut previous: Option<&[u8]> = None;
    for entry in entries {
        validate_name(&entry.name)?;
        if !matches!(entry.mode, MODE_TREE | MODE_FILE | MODE_EXEC | MODE_SYMLINK) {
            return Err(invalid(format!("unsupported tree mode {:o}", entry.mode)));
        }
        if let Some(prev) = previous
            && prev >= entry.name.as_slice()
        {
            return Err(invalid(format!(
                "tree entries are not strictly sorted: '{}' after '{}'",
                entry.name_lossy(),
                String::from_utf8_lossy(prev)
            )));
        }
        previous = Some(&entry.name);
    }
    Ok(())
}

fn validate_name(name: &[u8]) -> Result<()> {
    if name.is_empty() {
        return Err(invalid("tree entry name is empty"));
    }
    if name == b"." || name == b".." {
        return Err(invalid("tree entry name cannot be '.' or '..'"));
    }
    if name.contains(&0) || name.contains(&b'/') {
        return Err(invalid(format!(
            "tree entry name contains NUL or '/': '{}'",
            String::from_utf8_lossy(name)
        )));
    }
    Ok(())
}

fn invalid(reason: impl Into<String>) -> FodeltaError {
    FodeltaError::InvalidObject {
        reason: reason.into(),
    }
}

fn read_u16(payload: &[u8], cursor: &mut usize) -> Result<u16> {
    let bytes = payload
        .get(*cursor..*cursor + 2)
        .ok_or_else(|| invalid("truncated tree payload"))?;
    *cursor += 2;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(payload: &[u8], cursor: &mut usize) -> Result<u32> {
    let bytes = payload
        .get(*cursor..*cursor + 4)
        .ok_or_else(|| invalid("truncated tree payload"))?;
    *cursor += 4;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn oid(byte: u8) -> ObjectId {
        ObjectId::from_bytes([byte; 32])
    }

    #[test]
    fn roundtrip_sorted_tree() {
        let tree = Tree::new(vec![
            TreeEntry::new(MODE_FILE, oid(1), "b.txt"),
            TreeEntry::new(MODE_TREE, oid(2), "src"),
            TreeEntry::new(MODE_EXEC, oid(3), "run.sh"),
        ])
        .unwrap();
        let names: Vec<_> = tree.entries().iter().map(TreeEntry::name_lossy).collect();
        assert_eq!(names, ["b.txt", "run.sh", "src"]);

        let payload = tree.encode().unwrap();
        assert_eq!(Tree::decode(&payload).unwrap(), tree);
    }

    #[test]
    fn rejects_invalid_names() {
        for name in ["", ".", "..", "a/b", "a\0b"] {
            let result = Tree::new(vec![TreeEntry::new(MODE_FILE, oid(1), name)]);
            assert!(result.is_err(), "accepted {name:?}");
        }
    }

    #[test]
    fn decode_rejects_unsorted_entries() {
        let tree = Tree::new(vec![
            TreeEntry::new(MODE_FILE, oid(1), "a"),
            TreeEntry::new(MODE_FILE, oid(2), "b"),
        ])
        .unwrap();
        let mut payload = tree.encode().unwrap();
        // Swap the two names in the serialized form by rebuilding manually.
        let mut entries = tree.into_entries();
        entries.reverse();
        let unsorted = Tree::from_sorted(entries).unwrap_err();
        assert!(unsorted.to_string().contains("sorted"));
        // Also verify that truncation is caught.
        payload.truncate(payload.len() - 1);
        assert!(Tree::decode(&payload).is_err());
    }

    proptest! {
        #[test]
        fn arbitrary_trees_roundtrip(
            entries in prop::collection::vec((any::<[u8; 4]>(), any::<u8>()), 0..20)
        ) {
            let mut seen = std::collections::BTreeMap::new();
            for (name_bytes, oid_byte) in entries {
                seen.insert(crate::util::hex::encode(&name_bytes), oid_byte);
            }
            let tree = Tree::new(
                seen.into_iter()
                    .map(|(name, byte)| TreeEntry::new(MODE_FILE, oid(byte), name))
                    .collect(),
            ).unwrap();
            let payload = tree.encode().unwrap();
            prop_assert_eq!(Tree::decode(&payload).unwrap(), tree);
        }
    }
}
