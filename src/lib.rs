use std::{collections::HashSet, path::PathBuf};
use glob::glob;

// pub mod async_binary_writer;
pub mod event;
pub mod json_lines;

pub fn osh_files() -> HashSet<PathBuf> {
    let home = home::home_dir().expect("no home dir found");
    let pattern = format!("{}/.osh/**/*.osh", home.to_str().expect(""));

    glob(&pattern)
        .expect("failed to read glob pattern")
        .filter_map(Result::ok)
        .filter_map(|path| path.canonicalize().ok())
        .collect()
}
