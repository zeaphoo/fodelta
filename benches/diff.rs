use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use fodelta::diff::text::{DiffAlgorithm, WhitespaceOptions, render_unified};

fn bench_text_diff(c: &mut Criterion) {
    let old: String = (0..5000).map(|index| format!("line {index}\n")).collect();
    let new = old.replacen("line 2500\n", "line TWO\n", 1);

    c.bench_function("render_unified_5k_lines", |b| {
        b.iter(|| {
            let (text, stats) = render_unified(
                black_box("a/file.txt"),
                black_box("b/file.txt"),
                black_box(old.as_bytes()),
                black_box(new.as_bytes()),
                black_box(3),
                DiffAlgorithm::Myers,
                WhitespaceOptions::default(),
            );
            black_box((text.len(), stats.added, stats.removed));
        });
    });
}

criterion_group!(benches, bench_text_diff);
criterion_main!(benches);
