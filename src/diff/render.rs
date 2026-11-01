//! Render a single file change as a git-style patch, or collect its stats.

use crate::Result;
use crate::diff::text::{self, DiffAlgorithm, LineStats, WhitespaceOptions};
use crate::diff::{DiffSide, FileChange, FileChangeKind};
use crate::odb::{MODE_FILE, ObjectId};
use crate::util::path;

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub context: usize,
    pub algorithm: DiffAlgorithm,
    pub max_inline_bytes: usize,
    pub whitespace: WhitespaceOptions,
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
    if change.kind == FileChangeKind::TypeChanged {
        let deleted = FileChange {
            path: change.path.clone(),
            old_path: None,
            kind: FileChangeKind::Deleted,
            similarity: None,
            old: change.old,
            new: None,
        };
        let added = FileChange {
            path: change.path.clone(),
            old_path: None,
            kind: FileChangeKind::Added,
            similarity: None,
            old: None,
            new: change.new,
        };
        let deleted = render(&deleted, left, right, options)?;
        let added = render(&added, left, right, options)?;
        let mut stats = LineStats::default();
        for rendered in [&deleted, &added] {
            if let Some(part) = rendered.stats {
                stats.added += part.added;
                stats.removed += part.removed;
            }
        }
        let binary = deleted.binary || added.binary;
        let mut text = deleted.text;
        text.push_str(&added.text);
        return Ok(Rendered {
            text,
            stats: Some(stats),
            binary,
        });
    }

    let old_path = change.old_path.as_deref().unwrap_or(&change.path);
    let old_display = path::display(old_path);
    let new_display = path::display(&change.path);
    let old_info = change.old;
    let new_info = change.new;

    let mut out = format!("diff --git a/{old_display} b/{new_display}\n");

    match change.kind {
        FileChangeKind::Renamed => {
            if let (Some(old), Some(new)) = (old_info, new_info)
                && old.mode != new.mode
            {
                out.push_str(&format!(
                    "old mode {:o}\nnew mode {:o}\n",
                    old.mode, new.mode
                ));
            }
            out.push_str(&format!(
                "similarity index {}%\n",
                change.similarity.unwrap_or(100)
            ));
            out.push_str(&format!("rename from {old_display}\n"));
            out.push_str(&format!("rename to {new_display}\n"));
            return Ok(Rendered {
                text: out,
                stats: Some(LineStats::default()),
                binary: false,
            });
        }
        FileChangeKind::Added => {
            if let Some(new) = new_info {
                out.push_str(&format!("new file mode {:o}\n", new.mode));
            }
        }
        FileChangeKind::Deleted => {
            if let Some(old) = old_info {
                out.push_str(&format!("deleted file mode {:o}\n", old.mode));
            }
        }
        FileChangeKind::TypeChanged => {
            if let Some(old) = old_info {
                out.push_str(&format!("old mode {:o}\n", old.mode));
            }
            if let Some(new) = new_info {
                out.push_str(&format!("new mode {:o}\n", new.mode));
            }
        }
        FileChangeKind::Modified => {
            if let (Some(old), Some(new)) = (old_info, new_info)
                && old.mode != new.mode
            {
                out.push_str(&format!(
                    "old mode {:o}\nnew mode {:o}\n",
                    old.mode, new.mode
                ));
            }
        }
    }

    let old_oid = old_info.map(|info| info.oid).unwrap_or(ObjectId::ZERO);
    let new_oid = new_info.map(|info| info.oid).unwrap_or(ObjectId::ZERO);
    let mode = new_info
        .map(|info| info.mode)
        .or_else(|| old_info.map(|info| info.mode))
        .unwrap_or(MODE_FILE);
    out.push_str(&format!(
        "index {}..{} {:o}\n",
        old_oid.short(),
        new_oid.short(),
        mode
    ));

    if change.kind == FileChangeKind::TypeChanged {
        return Ok(Rendered {
            text: out,
            stats: Some(LineStats::default()),
            binary: false,
        });
    }

    let old = match old_info {
        Some(info) => left.read(old_path, &info)?,
        None => Vec::new(),
    };
    let new = match new_info {
        Some(info) => right.read(&change.path, &info)?,
        None => Vec::new(),
    };

    let old_label = if old_info.is_some() {
        format!("a/{old_display}")
    } else {
        "/dev/null".to_string()
    };
    let new_label = if new_info.is_some() {
        format!("b/{new_display}")
    } else {
        "/dev/null".to_string()
    };

    if text::is_binary(&old)
        || text::is_binary(&new)
        || old.len() > options.max_inline_bytes
        || new.len() > options.max_inline_bytes
    {
        out.push_str(&format!(
            "Binary files {old_label} and {new_label} differ\n"
        ));
        return Ok(Rendered {
            text: out,
            stats: None,
            binary: true,
        });
    }

    let (body, stats) = text::render_unified(
        &old_label,
        &new_label,
        &old,
        &new,
        options.context,
        options.algorithm,
        options.whitespace,
    );

    if stats.added == 0
        && stats.removed == 0
        && change.kind == FileChangeKind::Modified
        && old_info.map(|info| info.mode) == new_info.map(|info| info.mode)
    {
        return Ok(Rendered {
            text: String::new(),
            stats: Some(stats),
            binary: false,
        });
    }

    out.push_str(&body);
    Ok(Rendered {
        text: out,
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

    fn options() -> RenderOptions {
        RenderOptions {
            context: 3,
            algorithm: DiffAlgorithm::Myers,
            max_inline_bytes: 1024,
            whitespace: WhitespaceOptions::default(),
        }
    }

    #[test]
    fn added_text_file_has_git_headers_and_hunk() {
        let dir = tempfile::tempdir().unwrap();
        let repo = seed(dir.path());
        let store = repo.store();
        let oid = store.write_blob(b"hello\n").unwrap();
        let change = FileChange {
            path: b"a.txt".to_vec(),
            old_path: None,
            kind: FileChangeKind::Added,
            similarity: None,
            old: None,
            new: Some(info(MODE_FILE, oid)),
        };
        let side = DiffSide {
            repo: &repo,
            map: Default::default(),
            kind: SideKind::Store,
        };
        let rendered = render(&change, &side, &side, &options()).unwrap();
        assert!(rendered.text.contains("diff --git a/a.txt b/a.txt"));
        assert!(rendered.text.contains("new file mode 100644"));
        assert!(rendered.text.contains("--- /dev/null"));
        assert!(rendered.text.contains("+++ b/a.txt"));
        assert!(rendered.text.contains("+hello"));
        assert_eq!(rendered.stats.unwrap().added, 1);
    }

    #[test]
    fn binary_and_type_changes_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let repo = seed(dir.path());
        let store = repo.store();
        let oid = store.write_blob(b"a\0b").unwrap();
        let change = FileChange {
            path: b"logo.png".to_vec(),
            old_path: None,
            kind: FileChangeKind::Modified,
            similarity: None,
            old: Some(info(MODE_FILE, oid)),
            new: Some(info(MODE_FILE, oid)),
        };
        let side = DiffSide {
            repo: &repo,
            map: Default::default(),
            kind: SideKind::Store,
        };
        let rendered = render(&change, &side, &side, &options()).unwrap();
        assert!(rendered.binary);
        assert!(
            rendered
                .text
                .contains("Binary files a/logo.png and b/logo.png differ")
        );

        let old_blob = store.write_blob(b"data\n").unwrap();
        let new_blob = store.write_blob(b"target").unwrap();
        let type_change = FileChange {
            path: b"cfg".to_vec(),
            old_path: None,
            kind: FileChangeKind::TypeChanged,
            similarity: None,
            old: Some(info(MODE_FILE, old_blob)),
            new: Some(info(MODE_SYMLINK, new_blob)),
        };
        let rendered = render(&type_change, &side, &side, &options()).unwrap();
        assert!(rendered.text.contains("deleted file mode 100644"));
        assert!(rendered.text.contains("new file mode 120000"));
    }

    #[test]
    fn large_inline_files_are_treated_as_binary() {
        let dir = tempfile::tempdir().unwrap();
        let repo = seed(dir.path());
        let store = repo.store();
        let oid = store.write_blob(b"hello\n").unwrap();
        let change = FileChange {
            path: b"big.txt".to_vec(),
            old_path: None,
            kind: FileChangeKind::Modified,
            similarity: None,
            old: Some(info(MODE_FILE, oid)),
            new: Some(info(MODE_FILE, oid)),
        };
        let side = DiffSide {
            repo: &repo,
            map: Default::default(),
            kind: SideKind::Store,
        };
        let mut options = options();
        options.max_inline_bytes = 2;
        let rendered = render(&change, &side, &side, &options).unwrap();
        assert!(rendered.binary);
    }

    #[test]
    fn whitespace_only_change_is_suppressed() {
        let dir = tempfile::tempdir().unwrap();
        let repo = seed(dir.path());
        let store = repo.store();
        let old = store.write_blob(b"one two\n").unwrap();
        let new = store.write_blob(b"one  two\n").unwrap();
        let change = FileChange {
            path: b"a.txt".to_vec(),
            old_path: None,
            kind: FileChangeKind::Modified,
            similarity: None,
            old: Some(info(MODE_FILE, old)),
            new: Some(info(MODE_FILE, new)),
        };
        let side = DiffSide {
            repo: &repo,
            map: Default::default(),
            kind: SideKind::Store,
        };
        let mut options = options();
        options.whitespace = WhitespaceOptions {
            ignore_all: true,
            ignore_change: false,
            ignore_eol: false,
        };
        let rendered = render(&change, &side, &side, &options).unwrap();
        assert!(rendered.text.is_empty());
    }
}
