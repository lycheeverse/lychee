use criterion::{Criterion, criterion_group, criterion_main};
use lychee_lib::InputContent;
use lychee_lib::extract::Extractor;
use std::hint::black_box;
use std::path::PathBuf;

fn extract(paths: &[PathBuf]) {
    for path in paths {
        let content: InputContent = path.try_into().unwrap();
        let extractor = Extractor::default();
        let extracted = extractor.extract(&content);
        println!("{}", extracted.len());
    }
}

fn benchmark(c: &mut Criterion) {
    // Currently Wikipedia's biggest featured article
    c.bench_function("extract from large docs", |b| {
        b.iter(|| {
            extract(black_box(&[
                PathBuf::from("../fixtures/bench/elvis.html"),
                PathBuf::from("../fixtures/bench/arch.html"),
            ]))
        })
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = benchmark
);
criterion_main!(benches);
