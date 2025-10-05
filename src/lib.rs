use glob::glob;
use std::{collections::HashSet, path::PathBuf};

pub mod event;
pub mod formats;

pub fn osh_files() -> HashSet<PathBuf> {
    let home = home::home_dir().expect("no home dir found");
    let pattern = format!("{}/.osh/**/*.osh", home.to_str().expect(""));

    glob(&pattern)
        .expect("failed to read glob pattern")
        .filter_map(Result::ok)
        .filter_map(|path| path.canonicalize().ok())
        .collect()
}
