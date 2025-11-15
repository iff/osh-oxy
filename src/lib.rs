use std::{collections::HashSet, path::PathBuf};

use glob::glob;

pub mod commands;
pub mod event;
pub mod formats;
pub mod ui;

pub fn osh_files(kind: formats::Kind) -> HashSet<PathBuf> {
    let home = home::home_dir().expect("no home dir found");
    let pattern = format!(
        "{}/.osh/**/*.{}",
        home.to_str().expect(""),
        kind.extension()
    );

    glob(&pattern)
        .expect("failed to read glob pattern")
        .filter_map(Result::ok)
        .filter_map(|path| path.canonicalize().ok())
        .collect()
}
