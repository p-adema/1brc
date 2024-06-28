use criterion::{black_box, criterion_group, criterion_main, Criterion};
use parse::worker::Line;
use rand::prelude::SliceRandom;
use std::io::BufRead;

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut lines = get_lines(1e5 as usize);
    lines.shuffle(&mut rand::thread_rng());
    let lines = lines.as_slice();

    c.bench_function("parse 100k", move |b| {
        b.iter(|| black_box(parse_lines(lines)))
    });
}

fn parse_lines(lines: &[Vec<u8>]) -> usize {
    for line in lines {
        black_box(Line::parse_bytes(line));
    }
    lines.len()
}

fn get_lines(num: usize) -> Vec<Vec<u8>> {
    std::io::BufReader::new(
        std::fs::File::open(
            "/Users/peter/Documents/Other/Rust/sandbox/data/measurements.nosync.txt",
        )
        .expect("Measurements should exist"),
    )
    .lines()
    .skip(20_000)
    .take(num)
    .map(|l| l.expect("Should be valid line").into_bytes())
    .collect::<Vec<Vec<u8>>>()
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
