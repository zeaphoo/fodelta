use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use fodelta::history::{CommitOptions, create_commit};
use fodelta::repo::Repository;
use fodelta::worktree::compute_status;

fn bench_status(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    for index in 0..1000 {
        std::fs::write(
            repo.root.join(format!("file-{index}.txt")),
            format!("contents {index}\n"),
        )
        .unwrap();
    }
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

    c.bench_function("status_1k_clean", |b| {
        b.iter(|| black_box(compute_status(&repo, false).unwrap().total_changes()));
    });
}

criterion_group!(benches, bench_status);
criterion_main!(benches);
