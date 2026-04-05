use criterion::{criterion_group, criterion_main, Criterion};
use std::path::Path;

fn bench_convert(c: &mut Criterion) {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.ulg");
    if !Path::new(fixture).exists() {
        eprintln!("Skipping benchmark: sample.ulg not available");
        return;
    }

    c.bench_function("convert_ulog_sample", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            flight_review::converter::convert_ulog(fixture, tmp.path()).unwrap();
        });
    });
}

criterion_group!(benches, bench_convert);
criterion_main!(benches);
