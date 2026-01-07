use std::hint::black_box;

use arbitrary::{Arbitrary, Unstructured};
use criterion::{Criterion, criterion_group, criterion_main};
use itertools::kmerge_by;
use osh_oxy::event::Event;
use rayon::slice::ParallelSliceMut;

fn create_test_events(size: usize) -> impl Iterator<Item = Event> {
    let data = &[0; 1000];
    let mut u = Unstructured::new(data);
    #[allow(clippy::unwrap_used)]
    (0..size).map(move |_| Event::arbitrary(&mut u).unwrap())
}

fn osh_file(events_per_file: usize, sorted: bool) -> Vec<Event> {
    let mut events = create_test_events(events_per_file).collect::<Vec<_>>();
    if sorted {
        #[allow(clippy::unwrap_used)]
        events.sort_by(|a, b| b.partial_cmp(a).unwrap());
        // TODO descending?
        // events.reverse();
    }
    events
}

fn create_presorted_files(
    num_files: usize,
    events_per_file: usize,
    sorted: bool,
) -> Vec<Vec<Event>> {
    (0..num_files)
        .map(|_| osh_file(events_per_file, sorted))
        .collect::<Vec<_>>()
}

fn benchmark_sort(c: &mut Criterion) {
    let mut group = c.benchmark_group("par_sort_unstable");

    for total_events in [500_000, 1_000_000].iter() {
        group.bench_with_input(format!("{total_events}_events"), total_events, |b, _| {
            b.iter_with_setup(
                || osh_file(*total_events, false),
                |all| {
                    let mut all_items: Vec<Event> = all.into_iter().collect();
                    all_items.par_sort_unstable_by(|a, b| b.cmp(a));
                    black_box(all_items)
                },
            );
        });
    }

    group.finish();
}

fn benchmark_kmerge(c: &mut Criterion) {
    let mut group = c.benchmark_group("kmerge_by");
    let num_files = 5;

    for total_events in [500_000, 1_000_000].iter() {
        let events_per_file = total_events / num_files;

        group.bench_with_input(format!("{total_events}_events"), total_events, |b, _| {
            b.iter_with_setup(
                || create_presorted_files(num_files, events_per_file, true),
                |presorted_files| {
                    let iterators = presorted_files.into_iter().map(|ev| ev.into_iter().rev());
                    let result: Vec<_> =
                        kmerge_by(iterators, |a: &Event, b: &Event| a > b).collect();
                    black_box(result)
                },
            );
        });
    }

    group.finish();
}

criterion_group!(benches, benchmark_sort, benchmark_kmerge,);
criterion_main!(benches);
