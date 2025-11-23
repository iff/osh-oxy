use std::{collections::HashSet, path::PathBuf};

use anyhow::anyhow;
use glob::glob;

pub mod commands;
pub mod event;
pub mod formats;
pub mod ui;

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
