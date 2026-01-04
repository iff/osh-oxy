use std::io::Write;

use chrono::Utc;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::load_sorted;

pub fn invoke() -> anyhow::Result<()> {
    let f = timeago::Formatter::new();
    let now = Utc::now().timestamp_millis();
    let formatted: String = load_sorted()?
        .par_iter()
        .map(|item| {
            let d = std::time::Duration::from_millis((now - item.endtimestamp()) as u64);
            format!("{} --- {}", f.convert(d), item.command)
        })
        .collect();

    std::io::stdout().write_all(formatted.as_bytes())?;
    Ok(())
}
