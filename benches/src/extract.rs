use criterion::{Criterion, criterion_group, criterion_main};
use lychee_lib::extract::Extractor;
use lychee_lib::{FileType, Input, InputContent};
use std::hint::black_box;

fn extract(inputs: &Vec<InputContent>) {
    for input in inputs {
        let extractor = Extractor::default();
        let extracted = extractor.extract(input);
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
    let mut inputs = vec![];

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    runtime.block_on(async {
        inputs = vec![
            Input::path_content("../fixtures/bench/elvis.html", None)
                .await
                .unwrap(),
            Input::path_content("../fixtures/bench/arch.html", None)
                .await
                .unwrap(),
        ];
    });

    c.bench_function("extract from large docs", |b| b.iter(|| extract(&inputs)));
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = benchmark, benchmark_input_content_creation
);
criterion_main!(benches);
