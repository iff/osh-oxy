use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures::future;
use osh_oxy::event::{load_osh_events, osh_files, EventFilter, Events};
use tokio_test::block_on;

fn benchmark_load_osh_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_osh_files");

    // TODO need standalone files or generate them on the fly
    let filter = EventFilter::new(None);
    let osh_files = osh_files();
    if osh_files.is_empty() {
        eprintln!("no .osh files found");
        return;
    }

    group.bench_function("load_all_files", |b| {
        b.iter(|| {
            let all_events = block_on(future::try_join_all(
                osh_files.iter().map(|f| load_osh_events(f, &filter)),
            ))
            .expect("failed to load all files");
            black_box(all_events.into_iter().flatten().collect::<Events>())
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_load_osh_files);
criterion_main!(benches);
