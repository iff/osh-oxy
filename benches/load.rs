use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use futures::future;
use osh_oxy::event::EventFilter;
use osh_oxy::formats::{Kind, json_lines, rmp};
use osh_oxy::osh_files;
use std::hint::black_box;
use tokio::runtime::Runtime;

fn benchmark_load_json_lines(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_osh_files");
    group.measurement_time(Duration::from_secs_f64(16.0));

    // TODO need standalone files or generate them on the fly
    let filter = EventFilter::new(None);
    let osh_files = osh_files(Kind::JsonLines);
    if osh_files.is_empty() {
        eprintln!("no .osh files found");
        return;
    }

    group.bench_function("load_all_files", |b| {
        b.to_async(Runtime::new().unwrap()).iter(|| async {
            let all_events = future::try_join_all(
                osh_files
                    .iter()
                    .map(|f| json_lines::load_osh_events(f, &filter)),
            )
            .await
            .expect("failed to load all files");
            black_box(all_events)
        });
    });

    group.finish();
}

fn benchmark_load_rmp(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_osh_files");
    group.measurement_time(Duration::from_secs_f64(16.0));

    // TODO need standalone files or generate them on the fly
    let filter = EventFilter::new(None);
    let osh_files = osh_files(Kind::Rmp);
    if osh_files.is_empty() {
        eprintln!("no .osh files found");
        return;
    }

    group.bench_function("load_all_files", |b| {
        b.to_async(Runtime::new().unwrap()).iter(|| async {
            let all_events =
                future::try_join_all(osh_files.iter().map(|f| rmp::load_osh_events(f, &filter)))
                    .await
                    .expect("failed to load all files");
            black_box(all_events)
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_load_json_lines, benchmark_load_rmp);
criterion_main!(benches);
