//! Render a single file change as a patch, or collect its stats.

use crate::Result;
use crate::diff::text::{self, DiffAlgorithm, LineStats};
use crate::diff::{DiffSide, FileChange, FileChangeKind};
use crate::util::path;

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub context: usize,
    pub algorithm: DiffAlgorithm,
    pub max_inline_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct Rendered {
    pub text: String,
    pub stats: Option<LineStats>,
    pub binary: bool,
}

pub fn render(
    change: &FileChange,
    left: &DiffSide<'_>,
    right: &DiffSide<'_>,
    options: &RenderOptions,
) -> Result<Rendered> {
    let label = path::display(&change.path);
    let mut prefix = String::new();

    match change.kind {
        FileChangeKind::Added => {
            if let Some(new) = change.new {
                prefix.push_str(&format!("new file mode {:o}\n", new.mode));
            }
        }
        FileChangeKind::Deleted => {
            if let Some(old) = change.old {
                prefix.push_str(&format!("deleted file mode {:o}\n", old.mode));
            }
        }
        FileChangeKind::TypeChanged => {
            if let Some(old) = change.old {
                prefix.push_str(&format!("deleted file mode {:o}\n", old.mode));
            }
            if let Some(new) = change.new {
                prefix.push_str(&format!("new file mode {:o}\n", new.mode));
            }
            return Ok(Rendered {
                text: prefix,
                stats: None,
                binary: false,
            });
        }
        FileChangeKind::Modified => {
            if let (Some(old), Some(new)) = (change.old, change.new)
                && old.mode != new.mode
            {
                prefix.push_str(&format!(
                    "old mode {:o}\nnew mode {:o}\n",
                    old.mode, new.mode
                ));
            }
        }
    }

    let old = match change.old {
        Some(info) => left.read(&change.path, &info)?,
        None => Vec::new(),
    };
    let new = match change.new {
        Some(info) => right.read(&change.path, &info)?,
        None => Vec::new(),
    };

    if text::is_binary(&old)
        || text::is_binary(&new)
        || old.len() > options.max_inline_bytes
        || new.len() > options.max_inline_bytes
    {
        prefix.push_str(&format!("Binary files a/{label} and b/{label} differ\n"));
        return Ok(Rendered {
            text: prefix,
            stats: None,
            binary: true,
        });
    }

    let (body, stats) = text::render_unified(
        &format!("a/{label}"),
        &format!("b/{label}"),
        &old,
        &new,
        options.context,
        options.algorithm,
    );
    prefix.push_str(&body);
    Ok(Rendered {
        text: prefix,
        stats: Some(stats),
        binary: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::SideKind;
    use crate::diff::text::DiffAlgorithm;
    use crate::history::TreeEntryInfo;
    use crate::odb::{MODE_FILE, MODE_SYMLINK, ObjectId};
    use crate::repo::Repository;

    fn seed(dir: &std::path::Path) -> Repository {
        Repository::init(dir).unwrap()
    }

    fn info(mode: u32, oid: ObjectId) -> TreeEntryInfo {
        TreeEntryInfo { mode, oid }
    }

    #[test]
    fn added_text_file_has_mode_and_hunk() {
        let dir = tempfile::tempdir().unwrap();
        let repo = seed(dir.path());
        let store = repo.store();
        let oid = store.write_blob(b"hello\n").unwrap();
        let change = FileChange {
            path: b"a.txt".to_vec(),
            kind: FileChangeKind::Added,
            old: None,
            new: Some(info(MODE_FILE, oid)),
        };
        let side = DiffSide {
            repo: &repo,
            map: Default::default(),
            kind: SideKind::Store,
        };
        let rendered = render(
            &change,
            &side,
            &side,
            &RenderOptions {
                context: 3,
                algorithm: DiffAlgorithm::Myers,
                max_inline_bytes: 1024,
            },
        )
        .unwrap();
        assert!(rendered.text.contains("new file mode 100644"));
        assert!(rendered.text.contains("+hello"));
        assert_eq!(rendered.stats.unwrap().added, 1);
    }

    #[test]
    fn large_inline_files_are_treated_as_binary() {
        let dir = tempfile::tempdir().unwrap();
        let repo = seed(dir.path());
        let store = repo.store();
        let oid = store.write_blob(b"hello\n").unwrap();
        let change = FileChange {
            path: b"big.txt".to_vec(),
            kind: FileChangeKind::Modified,
            old: Some(info(MODE_FILE, oid)),
            new: Some(info(MODE_FILE, oid)),
        };
        let side = DiffSide {
            repo: &repo,
            map: Default::default(),
            kind: SideKind::Store,
        };
        let rendered = render(
            &change,
            &side,
            &side,
            &RenderOptions {
                context: 3,
                algorithm: DiffAlgorithm::Myers,
                max_inline_bytes: 2,
            },
        )
        .unwrap();
        assert!(rendered.binary);
    }

    #[test]
    fn binary_and_type_changes_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let repo = seed(dir.path());
        let store = repo.store();
        let oid = store.write_blob(b"a\0b").unwrap();
        let change = FileChange {
            path: b"logo.png".to_vec(),
            kind: FileChangeKind::Modified,
            old: Some(info(MODE_FILE, oid)),
            new: Some(info(MODE_FILE, oid)),
        };
        let side = DiffSide {
            repo: &repo,
            map: Default::default(),
            kind: SideKind::Store,
        };
        let rendered = render(
            &change,
            &side,
            &side,
            &RenderOptions {
                context: 3,
                algorithm: DiffAlgorithm::Myers,
                max_inline_bytes: 1024,
            },
        )
        .unwrap();
        assert!(rendered.binary);
        assert!(rendered.text.contains("Binary files"));

        let type_change = FileChange {
            path: b"cfg".to_vec(),
            kind: FileChangeKind::TypeChanged,
            old: Some(info(MODE_FILE, ObjectId::from_bytes([1; 32]))),
            new: Some(info(MODE_SYMLINK, ObjectId::from_bytes([2; 32]))),
        };
        let rendered = render(
            &type_change,
            &side,
            &side,
            &RenderOptions {
                context: 3,
                algorithm: DiffAlgorithm::Myers,
                max_inline_bytes: 1024,
            },
        )
        .unwrap();
        assert!(rendered.text.contains("deleted file mode 100644"));
        assert!(rendered.text.contains("new file mode 120000"));
    }
}
