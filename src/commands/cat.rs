use crate::event::{Event, EventFilter, load_osh_events, osh_files};
use chrono::Utc;
use futures::future;
use itertools::{Either, Itertools, kmerge_by};

pub(crate) async fn invoke(session_id: Option<String>, unique: bool) -> anyhow::Result<()> {
    let filter = EventFilter::new(session_id);
    let all =
        future::try_join_all(osh_files().into_iter().map(|f| load_osh_events(f, &filter))).await?;

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
