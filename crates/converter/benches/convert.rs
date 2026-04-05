use criterion::{criterion_group, criterion_main, Criterion};
use std::path::Path;
use std::time::Duration;

fn bench_convert(c: &mut Criterion) {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.ulg");
    if !Path::new(fixture).exists() {
        eprintln!("Skipping benchmark: sample.ulg not available");
        return;
    }

    let mut group = c.benchmark_group("convert");
    // Give CI runners enough time to collect samples without warnings
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);

    group.bench_function("convert_ulog_sample", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            flight_review::converter::convert_ulog(fixture, tmp.path()).unwrap();
        });
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().without_plots();
    targets = bench_convert
}
criterion_main!(benches);
