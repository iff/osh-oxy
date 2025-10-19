use crate::event::{Event, EventFilter};
use crate::{formats::Kind, osh_files};
use crate::formats::json_lines::load_osh_events;
use chrono::Utc;
use futures::future;
use itertools::{Either, Itertools, kmerge_by};

pub async fn invoke(session_id: Option<String>, unique: bool) -> anyhow::Result<()> {
    let filter = EventFilter::new(session_id);
    let oshs = osh_files(Kind::JsonLines);
    let all = future::try_join_all(oshs.into_iter().map(|f| load_osh_events(f, &filter))).await?;

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
        // TODO missing preview cols
    }

    Ok(())
}
