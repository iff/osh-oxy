use std::{
    collections::HashSet,
    fs::File,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
};

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
pub mod matcher;
pub mod ui;

/// memory map `file`
pub fn mmap(file: &File) -> &'_ [u8] {
    #[allow(clippy::unwrap_used)]
    let len = file.metadata().unwrap().len();
    unsafe {
        let ptr = libc::mmap(
            std::ptr::null_mut(),
            len as libc::size_t,
            libc::PROT_READ,
            libc::MAP_SHARED,
            file.as_raw_fd(),
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

/// discover all osh files of `kind` under `root`, recursively.
fn discover_files(root: &Path, kind: &formats::Kind) -> anyhow::Result<HashSet<PathBuf>> {
    let pattern = format!(
        "{}/**/*.{}",
        root.to_str()
            .ok_or(anyhow!("root path contains invalid chars"))?,
        kind.extension()
    );
    let files = match glob(&pattern) {
        Err(_) => unreachable!("pattern is valid"),
        Ok(matches) => matches
            .filter_map(Result::ok)
            .filter_map(|path| path.canonicalize().ok())
            .collect(),
    };
    Ok(files)
}

/// discover all parsable osh files under `~/.osh` for a specific format
pub fn osh_files(kind: formats::Kind) -> anyhow::Result<HashSet<PathBuf>> {
    let home_dir = home::home_dir().ok_or(anyhow!("no home directory"))?;
    discover_files(&home_dir.join(".osh"), &kind)
}

/// load all binary osh files in `~/.osh` and return a merged and sorted vector of all events
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

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn mmap_reads_file_contents() {
        let mut file = tempfile::tempfile().unwrap();
        let data = b"hello mmap";
        file.write_all(data).unwrap();
        let mapped = mmap(&file);
        assert_eq!(mapped, data);
    }

    #[test]
    fn discover_mixed_extensions() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("machine1")).unwrap();
        std::fs::File::create(dir.path().join("machine1/history.bosh")).unwrap();
        std::fs::File::create(dir.path().join("machine1/history.osh")).unwrap();

        let found = discover_files(dir.path(), &Kind::Rmp).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found.iter().all(|p| p.extension().unwrap() == "bosh"));
    }

    #[test]
    fn discover_empty_dir() {
        let dir = TempDir::new().unwrap();
        let found = discover_files(dir.path(), &Kind::Rmp).unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn discover_nested() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::File::create(dir.path().join("a/one.bosh")).unwrap();
        std::fs::File::create(dir.path().join("a/b/two.bosh")).unwrap();

        let found = discover_files(dir.path(), &Kind::Rmp).unwrap();
        assert_eq!(found.len(), 2);
    }
}
