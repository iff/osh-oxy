use crate::event::{Event, EventFilter};
use crate::formats::{Kind, json_lines};
use crate::osh_files;
use futures::future;
use itertools::{Either, Itertools, kmerge_by};
use skim::{
    RankCriteria, Skim, SkimItemReceiver, SkimItemSender,
    prelude::{Key, SkimOptionsBuilder, unbounded},
};
use std::{sync::Arc, thread};

pub async fn invoke(query: &str, session_id: Option<String>, unique: bool) -> anyhow::Result<()> {
    let oshs = osh_files(Kind::JsonLines);

    // TODO filter here and in parallel?
    let filter = EventFilter::new(session_id);
    let all = future::try_join_all(
        oshs.into_iter()
            .map(|f| json_lines::load_osh_events(f, &filter)),
    )
    .await?;

    let options = SkimOptionsBuilder::default()
        .height(String::from("70%"))
        .min_height(String::from("10"))
        .header(Some(String::from("osh-oxy")))
        .tiebreak(vec![RankCriteria::Index])
        .preview_window(String::from("down:5:wrap"))
        .preview(Some(String::new()))
        .multi(false)
        .query(Some(query.to_string()))
        .bind(vec![
            String::from("Enter:accept"),
            String::from("esc:abort"),
            String::from("ctrl-c:abort"),
        ])
        .build()?;

    let (tx_item, rx_item): (SkimItemSender, SkimItemReceiver) = unbounded();

    thread::spawn(move || {
        let iterators = all.into_iter().map(|ev| ev.into_iter().rev());
        let items = if unique {
            // FIXME keeps oldest when unique
            Either::Left(
                kmerge_by(iterators, |a: &Event, b: &Event| a > b)
                    .unique_by(|e: &Event| e.command.to_owned()),
            )
        } else {
            Either::Right(kmerge_by(iterators, |a: &Event, b: &Event| a > b))
        };
        for item in items {
            let _ = tx_item.send(Arc::new(item));
        }

        // notify skim to stop waiting for more
        drop(tx_item);
    });

    if let Some(out) = Skim::run_with(&options, Some(rx_item)) {
        match out.final_key {
            Key::ESC | Key::Ctrl('c') => return Ok(()),
            Key::Enter => {
                let item = out
                    .selected_items
                    .first()
                    .ok_or(anyhow::anyhow!("nothing selected"))?;
                println!("{}", item.output());
            }
            _ => (),
        }
    };

    Ok(())
}
