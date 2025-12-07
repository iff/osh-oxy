use chrono::Utc;
use futures::future;
use itertools::{Either, Itertools, kmerge_by};

use crate::{
    event::Event,
    formats::{Kind, json_lines::load_osh_events},
    osh_files,
};

pub async fn invoke(unique: bool) -> anyhow::Result<()> {
    let oshs = osh_files(Kind::JsonLines)?;
    let all = future::try_join_all(oshs.into_iter().map(load_osh_events)).await?;

    let iterators = all.into_iter().map(|ev| ev.into_iter().rev());
    let items = if unique {
        Either::Left(
            kmerge_by(iterators, |a: &Event, b: &Event| a > b)
                .unique_by(|e: &Event| e.command.to_owned()),
        )
    } else {
        Either::Right(kmerge_by(iterators, |a: &Event, b: &Event| a > b))
    };

    let f = timeago::Formatter::new();
    let now = Utc::now().timestamp_millis();
    for item in items {
        let d = std::time::Duration::from_millis((now - item.endtimestamp()) as u64);
        let ago = f.convert(d);
        println!("{ago} --- {}", item.command);
    }

    Ok(())
}
