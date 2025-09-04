use criterion::{Criterion, criterion_group, criterion_main};
use lychee_lib::extract::Extractor;
use lychee_lib::{FileType, InputContent};
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

fn benchmark_input_content_creation(c: &mut Criterion) {
    let test_data = "https://example.com/link1 https://example.com/link2 https://example.com/link3"
        .repeat(1000);
    let owned_string = test_data.clone();

    c.bench_function("InputContent::from_string", |b| {
        b.iter(|| InputContent::from_string(black_box(&test_data), FileType::Markdown))
    });

    c.bench_function("InputContent::from_owned_string (optimized)", |b| {
        b.iter(|| InputContent::from_str(black_box(owned_string.clone()), FileType::Markdown))
    });

    c.bench_function("InputContent::from_string with owned (baseline)", |b| {
        b.iter(|| {
            let owned_string = test_data.clone();
            InputContent::from_string(black_box(&owned_string), FileType::Markdown)
        })
    });
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
    targets = benchmark, benchmark_input_content_creation
);
criterion_main!(benches);
