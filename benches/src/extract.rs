use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lychee_lib::extract::Extractor;
use lychee_lib::{FileType, InputContent};
use std::fs;

fn extract(input: &str) {
    Extractor::extract(&InputContent::from_string(input, FileType::Html));
}

fn benchmark(c: &mut Criterion) {
    // Currently Wikipedia's biggest featured article
    let elvis = fs::read_to_string("../fixtures/elvis.html").unwrap();
    c.bench_function("extract from large doc", |b| {
        b.iter(|| extract(black_box(&elvis)))
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = benchmark
);
criterion_main!(benches);
