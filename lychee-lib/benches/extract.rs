use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lychee_lib::extract::Extractor;
use lychee_lib::{FileType, InputContent};
use std::fs;

fn extract(input: &str) {
    let mut extractor = Extractor::new(None);
    extractor.extract(&InputContent::from_string(input, FileType::Html)).unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
    // Currently Wikipedia's biggest featured article
    let elvis = fs::read_to_string("../fixtures/elvis.html").unwrap();
    c.bench_function("extract from large doc", |b| {
        b.iter(|| extract(black_box(&elvis)))
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = criterion_benchmark
);
criterion_main!(benches);
