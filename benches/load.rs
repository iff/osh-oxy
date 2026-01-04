use std::{fs::File, hint::black_box, time::Duration};

use criterion::{Criterion, criterion_group, criterion_main};
use osh_oxy::{
    event::Event,
    formats::{Kind, rmp},
    mmap, osh_files,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

#[allow(clippy::expect_used)]
fn benchmark_load_rmp(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_osh_files");
    group.measurement_time(Duration::from_secs_f64(16.0));

    let oshs = osh_files(Kind::Rmp).expect("osh files should load");
    let osh_files: Vec<File> = oshs
        .iter()
        .map(|o| File::open(o).expect("open file"))
        .collect();
    let oshs_data: Vec<&[u8]> = osh_files.iter().map(mmap).collect();

    group.bench_function("load_all_files", |_| {
        let all_events = oshs_data
            .par_iter()
            .map(|d| rmp::load_osh_events(d).expect("load events"))
            .collect::<Vec<Vec<Event>>>();
        black_box(all_events);
    });

    group.finish();
}

criterion_group!(benches, benchmark_load_rmp);
criterion_main!(benches);
