//! Reachability based garbage collection.

use std::time::SystemTime;

use crate::Result;
use crate::maintenance::reach::{collect_roots, mark_reachable};
use crate::repo::Repository;

#[derive(Debug, Clone)]
pub struct GcOutcome {
    pub reachable: usize,
    pub dangling: usize,
    pub removed: usize,
    pub bytes: u64,
    pub dry_run: bool,
}

pub fn gc(repo: &Repository, dry_run: bool) -> Result<GcOutcome> {
    let _lock = repo.lock()?;
    let roots = collect_roots(repo, Some(repo.config.core.safety_retention_days))?;
    let store = repo.store();
    let reachable = mark_reachable(&store, &roots)?;
    let all = store.iter_oids()?;
    let dangling: Vec<_> = all
        .into_iter()
        .filter(|oid| !reachable.contains(oid))
        .collect();

    let prune_seconds = repo.config.core.prune_age_days.saturating_mul(86_400);
    let now = SystemTime::now();
    let mut removed = 0;
    let mut bytes = 0u64;
    for oid in &dangling {
        let path = store.path_for(*oid);
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let age = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        if age < prune_seconds {
            continue;
        }
        if !dry_run {
            std::fs::remove_file(&path)?;
        }
        removed += 1;
        bytes += metadata.len();
        if !dry_run && let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }

    if !dry_run {
        cleanup_temp_files(store.root())?;
        crate::safety::prune_log(repo, repo.config.core.safety_retention_days)?;
    }

    Ok(GcOutcome {
        reachable: reachable.len(),
        dangling: dangling.len(),
        removed,
        bytes,
        dry_run,
    })
}

fn cleanup_temp_files(root: &std::path::Path) -> Result<()> {
    cleanup_temp_dir(root)?;
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            cleanup_temp_dir(&entry.path())?;
        }
    }
    Ok(())
}

fn cleanup_temp_dir(directory: &std::path::Path) -> Result<()> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let now = SystemTime::now();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(".fodelta-tmp-") {
            continue;
        }
        let metadata = entry.metadata()?;
        let old = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age.as_secs() >= 86_400);
        if old {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{CommitOptions, create_commit};

    #[test]
    fn removes_old_unreachable_objects() {
        use std::time::{Duration, SystemTime};

        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let orphan = repo.store().write_blob(b"orphan").unwrap();
        let path = repo.store().path_for(orphan);
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(
            std::fs::FileTimes::new()
                .set_modified(SystemTime::now() - Duration::from_secs(20 * 86_400)),
        )
        .unwrap();
        drop(file);

        let outcome = gc(&repo, false).unwrap();
        assert!(outcome.removed >= 1);
        assert!(!repo.store().exists(orphan));
        assert!(
            crate::maintenance::fsck::fsck(&repo, false)
                .unwrap()
                .is_healthy()
        );
    }

    #[test]
    fn keeps_reachable_objects_and_reports_dangling() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        std::fs::write(repo.root.join("a.txt"), b"a").unwrap();
        create_commit(
            &repo,
            &CommitOptions {
                message: "seed".to_string(),
                allow_empty: false,
                allow_large_files: false,
                paths: Vec::new(),
            },
        )
        .unwrap();
        let orphan = repo.store().write_blob(b"orphan").unwrap();

        let outcome = gc(&repo, true).unwrap();
        assert!(outcome.reachable >= 3);
        assert!(outcome.dangling >= 1);
        assert!(repo.store().exists(orphan));
        assert_eq!(outcome.removed, 0);
    }
}
