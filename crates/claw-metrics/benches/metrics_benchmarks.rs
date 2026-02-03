//! Benchmarks for claw-metrics.

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use claw_metrics::{MetricName, MetricPoint, MetricStore, TimeRange};

fn benchmark_push(c: &mut Criterion) {
    let store = MetricStore::new(Duration::from_secs(3600));
    let name = MetricName::new("benchmark_metric").unwrap();

    c.bench_function("push_single_point", |b| {
        b.iter(|| {
            store
                .push(&name, MetricPoint::now(black_box(42.0)))
                .unwrap();
        });
    });
}

fn benchmark_push_with_labels(c: &mut Criterion) {
    let store = MetricStore::new(Duration::from_secs(3600));
    let name = MetricName::new("benchmark_labeled_metric").unwrap();

    c.bench_function("push_with_labels", |b| {
        b.iter(|| {
            store
                .push(
                    &name,
                    MetricPoint::now(black_box(42.0))
                        .label("gpu_id", "0")
                        .label("node_id", "node-1"),
                )
                .unwrap();
        });
    });
}

fn benchmark_query(c: &mut Criterion) {
    let store = MetricStore::new(Duration::from_secs(3600));
    let name = MetricName::new("benchmark_query_metric").unwrap();

    // Pre-populate with data
    let base_ts = MetricPoint::now_timestamp();
    for i in 0..10000 {
        let point = MetricPoint::new(base_ts - (10000 - i) * 100, i as f64);
        store.push(&name, point).unwrap();
    }

    let range = TimeRange::last_minutes(5);

    c.bench_function("query_10k_points", |b| {
        b.iter(|| {
            let _ = store.query(&name, black_box(range), None);
        });
    });
}

fn benchmark_push_batch(c: &mut Criterion) {
    let store = MetricStore::new(Duration::from_secs(3600));
    let name = MetricName::new("benchmark_batch_metric").unwrap();

    c.bench_function("push_batch_100", |b| {
        b.iter(|| {
            let metrics: Vec<_> = (0..100)
                .map(|i| (name.clone(), MetricPoint::now(black_box(i as f64))))
                .collect();
            store.push_batch(metrics).unwrap();
        });
    });
}

criterion_group!(
    benches,
    benchmark_push,
    benchmark_push_with_labels,
    benchmark_query,
    benchmark_push_batch,
);

criterion_main!(benches);
