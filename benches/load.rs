use std::{fs::File, hint::black_box, os::fd::AsRawFd, time::Duration};

use criterion::{Criterion, criterion_group, criterion_main};
use osh_oxy::{
    event::Event,
    formats::{Kind, rmp},
    osh_files,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

/// Copy of the crate private `osh_oxy::mmap` that unmaps on drop, so mapping inside every bench
/// iteration measures map + unmap instead of piling up mappings until `vm.max_map_count`.
struct Mapping {
    ptr: *mut libc::c_void,
    len: usize,
}

#[allow(clippy::expect_used)]
impl Mapping {
    fn new(file: &File) -> Self {
        let len = usize::try_from(file.metadata().expect("statx").len()).expect("size fits usize");
        assert!(len > 0, "mmap rejects empty files");
        // SAFETY: same reasoning as `osh_oxy::mmap`; the region is unmapped in `drop`
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        assert!(
            ptr != libc::MAP_FAILED,
            "{:?}",
            std::io::Error::last_os_error()
        );
        // SAFETY: `ptr` and `len` describe the mapping created above
        let advised = unsafe { libc::madvise(ptr, len, libc::MADV_SEQUENTIAL) };
        assert!(advised == 0, "{:?}", std::io::Error::last_os_error());
        Self { ptr, len }
    }

    fn as_slice(&self) -> &[u8] {
        // SAFETY: the mapping is readable for `len` bytes until `self` is dropped, and the
        // returned borrow cannot outlive `self`
        unsafe { std::slice::from_raw_parts(self.ptr.cast::<u8>(), self.len) }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: unmaps exactly the region created in `new`; no borrow from `as_slice` is alive
        let unmapped = unsafe { libc::munmap(self.ptr, self.len) };
        assert!(unmapped == 0, "{:?}", std::io::Error::last_os_error());
    }
}

#[allow(clippy::expect_used)]
fn benchmark_load_rmp(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_osh_files");
    group.measurement_time(Duration::from_secs_f64(16.0));

    let oshs = osh_files(&Kind::Rmp).expect("osh files should load");
    let osh_files: Vec<File> = oshs
        .iter()
        .map(|o| File::open(o).expect("open file"))
        .collect();

    group.bench_function("load_all_files", |b| {
        b.iter(|| {
            let mappings: Vec<Mapping> = osh_files.iter().map(Mapping::new).collect();
            let oshs_data: Vec<&[u8]> = mappings.iter().map(Mapping::as_slice).collect();
            black_box(
                oshs_data
                    .par_iter()
                    .map(|d| rmp::load_osh_events(d).expect("load events"))
                    .collect::<Vec<Vec<Event>>>(),
            );
        });
    });

    group.bench_function("load_all_files_sequential", |b| {
        b.iter(|| {
            let mappings: Vec<Mapping> = osh_files.iter().map(Mapping::new).collect();
            let oshs_data: Vec<&[u8]> = mappings.iter().map(Mapping::as_slice).collect();
            black_box(
                oshs_data
                    .iter()
                    .map(|d| rmp::load_osh_events(d).expect("load events"))
                    .collect::<Vec<Vec<Event>>>(),
            );
        });
    });

    group.bench_function("load_all_files_fs_read", |b| {
        b.iter(|| {
            let oshs_data: Vec<Vec<u8>> = oshs.iter().map(|f| std::fs::read(f).unwrap()).collect();
            black_box(
                oshs_data
                    .par_iter()
                    .map(|d| rmp::load_osh_events(d).expect("load events"))
                    .collect::<Vec<Vec<Event>>>(),
            );
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_load_rmp);
criterion_main!(benches);
