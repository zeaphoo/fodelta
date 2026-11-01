//! Resolve user supplied revision expressions.

use std::collections::BTreeSet;

use crate::odb::{Commit, ObjectId, ObjectKind};
use crate::repo::Repository;
use crate::{FodeltaError, Result};

/// Resolve `HEAD`, `@`, `main`, `HEAD~N`, full object ids and unique short
/// prefixes. Safety snapshots participate in prefix resolution even though they
/// are not on the main chain.
pub fn resolve(repo: &Repository, input: &str) -> Result<ObjectId> {
    let input = input.trim();
    if input.is_empty() {
        return Err(invalid(input, "empty revision"));
    }
    if matches!(input, "HEAD" | "@" | "main") {
        return repo.require_head();
    }

    if let Some(rest) = input
        .strip_prefix("HEAD~")
        .or_else(|| input.strip_prefix("@~"))
    {
        let generations: usize = rest
            .parse()
            .map_err(|_| invalid(input, "generation must be a non-negative number"))?;
        let mut oid = repo.require_head()?;
        for _ in 0..generations {
            let commit = read_commit(repo, oid)?;
            oid = commit
                .parent
                .ok_or_else(|| invalid(input, "revision walks past the root commit"))?;
        }
        return Ok(oid);
    }

    if (6..=64).contains(&input.len()) && input.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        let lower = input.to_ascii_lowercase();
        if input.len() == 64 {
            let oid = ObjectId::from_hex(&lower)?;
            if let Ok((kind, _)) = repo.store().read(oid)
                && kind == ObjectKind::Commit
            {
                return Ok(oid);
            }
        }
        let candidates = candidates(repo)?;
        let matches: Vec<ObjectId> = candidates
            .into_iter()
            .filter(|oid| oid.to_hex().starts_with(&lower))
            .collect();
        return match matches.as_slice() {
            [oid] => Ok(*oid),
            [] => Err(invalid(input, "no object matches this prefix")),
            many => Err(FodeltaError::AmbiguousRevision {
                rev: input.to_string(),
                candidates: many
                    .iter()
                    .map(|oid| oid.short())
                    .collect::<Vec<_>>()
                    .join(", "),
            }),
        };
    }

    Err(invalid(input, "unsupported revision syntax"))
}

pub fn read_commit(repo: &Repository, oid: ObjectId) -> Result<Commit> {
    let (kind, payload) = repo.store().read(oid)?;
    if kind != ObjectKind::Commit {
        return Err(FodeltaError::RepositoryCorrupt(format!(
            "object {} is not a commit",
            oid.short()
        )));
    }
    Commit::decode(&payload)
}

fn candidates(repo: &Repository) -> Result<BTreeSet<ObjectId>> {
    let mut out = BTreeSet::new();
    let mut current = repo.head_oid()?;
    while let Some(oid) = current {
        if !out.insert(oid) {
            break;
        }
        let commit = read_commit(repo, oid)?;
        current = commit.parent;
    }

    let safety = repo.fodelta.join("logs/safety");
    if let Ok(text) = std::fs::read_to_string(safety) {
        for line in text.lines() {
            let Some(first) = line.split_whitespace().next() else {
                continue;
            };
            if let Ok(oid) = ObjectId::from_hex(first) {
                out.insert(oid);
            }
        }
    }
    Ok(out)
}

fn invalid(rev: &str, reason: &str) -> FodeltaError {
    FodeltaError::InvalidRevision {
        rev: rev.to_string(),
        reason: reason.to_string(),
    }
}
