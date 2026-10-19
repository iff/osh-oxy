use std::io::Write;

use chrono::Utc;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::load_sorted;

/// # Errors
///
/// Returns an `Err` if loading or writing to stdout fails.
pub fn invoke() -> anyhow::Result<()> {
    let f = timeago::Formatter::new();
    let now = Utc::now().timestamp_millis();
    let formatted: String = load_sorted()?
        .par_iter()
        .map(|item| {
            format!(
                "{} --- {}\n",
                f.convert(item.elapsed_since_end(now)),
                item.command
            )
        })
        .collect();

    std::io::stdout().write_all(formatted.as_bytes())?;
    Ok(())
}
