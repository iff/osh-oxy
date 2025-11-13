use std::{sync::Arc, thread};

use futures::future;
use itertools::kmerge_by;

use crate::{
    event::Event,
    formats::{Kind, json_lines},
    osh_files,
    ui::Tui,
};

pub async fn invoke(query: &str, folder: &str, session_id: Option<String>) -> anyhow::Result<()> {
    let oshs = osh_files(Kind::JsonLines);

    let all = future::try_join_all(oshs.into_iter().map(json_lines::load_osh_events)).await?;

    let (tx_item, receiver) = crossbeam_channel::unbounded();
    thread::spawn(move || {
        let iterators = all.into_iter().map(|ev| ev.into_iter().rev());
        for item in kmerge_by(iterators, |a: &Event, b: &Event| a > b) {
            let _ = tx_item.send(Arc::new(item));
        }

        // notify skim to stop waiting for more
        drop(tx_item);
    });

    let selected = Tui::start(receiver, query, folder, session_id);
    if let Some(event) = selected {
        println!("{}", event.command);
    }
    Ok(())
}
