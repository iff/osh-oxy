use std::{sync::Arc, thread};

use futures::future;
use itertools::{Either, Itertools, kmerge_by};

use crate::{
    event::Event,
    formats::{Kind, json_lines},
    osh_files, ui,
};

pub async fn invoke(_query: &str, session_id: Option<String>) -> anyhow::Result<()> {
    let oshs = osh_files(Kind::JsonLines);

    let all = future::try_join_all(oshs.into_iter().map(json_lines::load_osh_events)).await?;

    let (tx_item, receiver) = crossbeam_channel::unbounded();
    thread::spawn(move || {
        let iterators = all.into_iter().map(|ev| ev.into_iter().rev());
        let items = if false {
            // unique {
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

    let selected = ui::ui(receiver, session_id);
    if let Some(event) = selected {
        println!("{}", event.command);
    }
    Ok(())
}
