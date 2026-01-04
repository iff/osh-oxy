use std::{collections::HashSet, fs::File, os::fd::AsRawFd, path::PathBuf};

use anyhow::anyhow;
use glob::glob;
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    slice::ParallelSliceMut,
};

use crate::{
    event::Event,
    formats::{Kind, rmp},
};

pub mod commands;
pub mod event;
pub mod formats;
pub mod ui;

pub fn mmap(f: &File) -> &'_ [u8] {
    #[allow(clippy::unwrap_used)]
    let len = f.metadata().unwrap().len();
    unsafe {
        let ptr = libc::mmap(
            std::ptr::null_mut(),
            len as libc::size_t,
            libc::PROT_READ,
            libc::MAP_SHARED,
            f.as_raw_fd(),
            0,
        );
        if ptr == libc::MAP_FAILED {
            panic!("{:?}", std::io::Error::last_os_error());
        } else {
            if libc::madvise(ptr, len as libc::size_t, libc::MADV_SEQUENTIAL) != 0 {
                panic!("{:?}", std::io::Error::last_os_error())
            }
            std::slice::from_raw_parts(ptr as *const u8, len as usize)
        }
    }
}

pub fn osh_files(kind: formats::Kind) -> anyhow::Result<HashSet<PathBuf>> {
    // TODO when can this really fail?
    let home_dir = home::home_dir().ok_or(anyhow!("no home directory"))?;
    let home = home_dir
        .to_str()
        .ok_or(anyhow!("home directory contains invalid chars"))?;
    let pattern = format!("{home}/.osh/**/*.{}", kind.extension());

    let files = match glob(&pattern) {
        Err(_) => unreachable!("pattern is valid"),
        Ok(matches) => matches
            .filter_map(Result::ok)
            .filter_map(|path| path.canonicalize().ok())
            .collect(),
    };

    Ok(files)
}

pub fn load_sorted() -> anyhow::Result<Vec<Event>> {
    let oshs = osh_files(Kind::Rmp)?;
    let osh_files: Vec<File> = oshs
        .into_iter()
        .map(File::open)
        .collect::<Result<Vec<_>, _>>()?;
    let oshs_data: Vec<&[u8]> = osh_files.iter().map(mmap).collect();
    let all: Vec<Vec<Event>> = oshs_data
        .par_iter()
        .map(|data| rmp::load_osh_events(data))
        .collect::<Result<Vec<_>, _>>()?;

    let mut all_items: Vec<Event> = all.into_iter().flatten().collect();
    all_items.par_sort_unstable_by(|a, b| b.cmp(a));
    Ok(all_items)
}
