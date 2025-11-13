use chrono::Utc;
use futures::future;
use itertools::{Either, Itertools, kmerge_by};

use crate::{
    event::Event,
    formats::{Kind, json_lines::load_osh_events},
    osh_files,
};

pub async fn invoke(unique: bool) -> anyhow::Result<()> {
    let oshs = osh_files(Kind::JsonLines);
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

    for item in items {
        let f = timeago::Formatter::new();
        let ago = f.convert_chrono(item.endtime(), Utc::now());
        println!("{ago} --- {}", item.command);
    }

    Ok(())
}
