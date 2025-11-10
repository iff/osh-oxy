use arbitrary::{Arbitrary, Unstructured};
use criterion::{Criterion, criterion_group, criterion_main};
use futures::future;
use itertools::kmerge_by;
use osh_oxy::event::{Event, EventFilter, Events};
use osh_oxy::formats::{Kind, json_lines::load_osh_events};
use osh_oxy::osh_files;
use std::hint::black_box;
use tokio_test::block_on;

fn create_test_events(size: usize) -> impl Iterator<Item = Event> {
    // TODO size;
    let data = &[0; 1000];
    let mut u = Unstructured::new(data);
    (0..size).map(move |_| Event::arbitrary(&mut u).unwrap())
}

fn osh_file(events_per_file: usize, sorted: bool) -> Events {
    let mut events = create_test_events(events_per_file).collect::<Vec<_>>();
    if sorted {
        events.sort_by(|a, b| b.partial_cmp(a).unwrap());
        // TODO descending?
        // events.reverse();
    }
    events
}

fn create_presorted_files(num_files: usize, events_per_file: usize, sorted: bool) -> Vec<Events> {
    (0..num_files)
        .map(|_| osh_file(events_per_file, sorted))
        .collect::<Vec<_>>()
}

fn benchmark_sort_real_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("sort_by_timestamp");

    let osh_files = osh_files(Kind::JsonLines);
    if osh_files.is_empty() {
        eprintln!("no .osh files found");
        return;
    }

    let filter = EventFilter::new(None);
    let all = block_on(future::try_join_all(
        osh_files.into_iter().map(|f| load_osh_events(f, &filter)),
    ))
    .unwrap();
    let mut all = all.into_iter().flatten().collect::<Events>();

    group.bench_function("real osh data", |b| {
        b.iter(|| {
            all.sort_by(|a, b| black_box(b.partial_cmp(a).unwrap()));
            black_box(all.clone())
        });
    });

    group.finish();
}

fn benchmark_kmerge_real_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("kmerge_real");

    let osh_files = osh_files(Kind::JsonLines);
    if osh_files.is_empty() {
        eprintln!("no .osh files found");
        return;
    }

    let filter = EventFilter::new(None);
    let all = block_on(future::try_join_all(
        osh_files.into_iter().map(|f| load_osh_events(f, &filter)),
    ))
    .unwrap();

    group.bench_function("real osh data", |b| {
        b.iter(|| {
            let iterators = all.clone().into_iter().map(|ev| ev.into_iter().rev());
            let result: Vec<_> = kmerge_by(iterators, |a: &Event, b: &Event| a > b).collect();
            black_box(result)
        });
    });

    group.finish();
}

fn benchmark_sort(c: &mut Criterion) {
    let mut group = c.benchmark_group("sort_by_timestamp");
    let num_files = 5;

    for total_events in [100_000, 200_000].iter() {
        for sorted in [true, false].iter() {
            let events_per_file = total_events / num_files;

            group.bench_with_input(
                format!("{total_events}_events_sorted_{sorted}"),
                total_events,
                |b, _| {
                    b.iter_with_setup(
                        || create_presorted_files(num_files, events_per_file, *sorted),
                        |presorted_files| {
                            let mut all_events: Events =
                                presorted_files.into_iter().flatten().collect();
                            all_events.sort_by(|a, b| black_box(b.partial_cmp(a).unwrap()));
                            black_box(all_events)
                        },
                    );
                },
            );
        }
    }

    group.finish();
}

fn benchmark_sort_unstable(c: &mut Criterion) {
    let mut group = c.benchmark_group("sort_by_unstable_timestamp");

    let num_files = 5;
    let total_events = 100_000;
    let events_per_file = total_events / num_files;

    group.bench_with_input(format!("{}_events", total_events), &total_events, |b, _| {
        b.iter_with_setup(
            || create_presorted_files(num_files, events_per_file, true),
            |presorted_files| {
                let mut all_events: Events = presorted_files.into_iter().flatten().collect();
                all_events.sort_unstable_by(|a, b| black_box(b.partial_cmp(a).unwrap()));
                black_box(all_events)
            },
        );
    });

    group.finish();
}

fn benchmark_kmerge(c: &mut Criterion) {
    let mut group = c.benchmark_group("kmerge_arbitrary");
    let num_files = 5;

    for total_events in [100_000, 200_000].iter() {
        let events_per_file = total_events / num_files;

        group.bench_with_input(
            format!("{total_events}_events_presorted"),
            total_events,
            |b, _| {
                b.iter_with_setup(
                    || create_presorted_files(num_files, events_per_file, true),
                    |presorted_files| {
                        let iterators = presorted_files.into_iter().map(|ev| ev.into_iter().rev());
                        let result: Vec<_> =
                            kmerge_by(iterators, |a: &Event, b: &Event| a > b).collect();
                        black_box(result)
                    },
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_sort_real_data,
    benchmark_kmerge_real_data,
    benchmark_sort,
    benchmark_sort_unstable,
    benchmark_kmerge,
);
criterion_main!(benches);
