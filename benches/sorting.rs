use std::{cmp::Reverse, hint::black_box, num::NonZeroU64};

use criterion::{Criterion, criterion_group, criterion_main};
use itertools::kmerge_by;
use osh_oxy::event::Event;
use rayon::slice::ParallelSliceMut;

const NUM_FILES: u64 = 5;

/// Generates a osh history file. Runs are reproducible and files from different seeds interleave in time.
#[allow(clippy::expect_used)]
fn osh_file(events_per_file: usize, seed: NonZeroU64) -> Vec<Event> {
    let mut state: u64 = seed.into();
    let mut endtime = 1_700_000_000_000_i64;
    (0..events_per_file)
        .map(|index| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // up to a minute between two commands finishing
            endtime += i64::try_from(state % 60_000).expect("gap fits i64");
            Event {
                timestamp_millis: endtime - 1_000,
                command: format!("command {}", index % 1_000),
                endtime,
                exit_code: 0,
                folder: "/".to_string(),
                machine: format!("machine {seed}"),
                session: format!("session {}", index / 100),
            }
        })
        .collect()
}

#[allow(clippy::expect_used)]
fn osh_files(total_events: usize) -> Vec<Vec<Event>> {
    let events_per_file = total_events / usize::try_from(NUM_FILES).expect("fits usize");
    (0..NUM_FILES)
        .map(|seed| osh_file(events_per_file, NonZeroU64::new(seed + 1).unwrap()))
        .collect()
}

/// what `load_sorted` sorts: all files concatenated
fn concatenated_files(total_events: usize) -> Vec<Event> {
    osh_files(total_events).into_iter().flatten().collect()
}

fn benchmark_sort(c: &mut Criterion) {
    let mut group = c.benchmark_group("par_sort_unstable");

    for total_events in &[200_000, 500_000, 1_000_000] {
        group.bench_with_input(format!("{total_events}_events"), total_events, |b, _| {
            b.iter_with_setup(
                || concatenated_files(*total_events),
                |mut all_items: Vec<Event>| {
                    all_items.par_sort_unstable_by_key(|event| Reverse(event.endtime));
                    black_box(all_items)
                },
            );
        });
    }

    group.finish();
}

fn benchmark_sort_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("sort_unstable");

    for total_events in &[200_000, 500_000, 1_000_000] {
        group.bench_with_input(format!("{total_events}_events"), total_events, |b, _| {
            b.iter_with_setup(
                || concatenated_files(*total_events),
                |mut all_items: Vec<Event>| {
                    all_items.sort_unstable_by_key(|event| Reverse(event.endtime));
                    black_box(all_items)
                },
            );
        });
    }

    group.finish();
}

fn benchmark_kmerge(c: &mut Criterion) {
    let mut group = c.benchmark_group("kmerge_by");

    for total_events in &[500_000, 1_000_000] {
        group.bench_with_input(format!("{total_events}_events"), total_events, |b, _| {
            b.iter_with_setup(
                || osh_files(*total_events),
                |files| {
                    let iterators = files.into_iter().map(|file| file.into_iter().rev());
                    let result: Vec<_> =
                        kmerge_by(iterators, |a: &Event, b: &Event| a.endtime > b.endtime)
                            .collect();
                    black_box(result)
                },
            );
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_sort,
    benchmark_sort_sequential,
    benchmark_kmerge,
);
criterion_main!(benches);
