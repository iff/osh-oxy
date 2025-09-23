use futures::future;
use osh_oxy::{load_osh_events, osh_files, Events};
use skim::{
    prelude::{unbounded, SkimOptionsBuilder},
    RankCriteria, Skim, SkimItemReceiver, SkimItemSender,
};
use std::{sync::Arc, thread};

pub(crate) async fn invoke(
    query: &str,
    session_id: Option<String>,
    session_start: Option<f32>,
) -> anyhow::Result<()> {
    let oshs = osh_files();
    let mut all = future::try_join_all(oshs.into_iter().map(load_osh_events))
        .await?
        .into_iter()
        .flatten()
        .collect::<Events>();

    let options = SkimOptionsBuilder::default()
        .height(String::from("70%"))
        .min_height(String::from("10"))
        .header(Some(String::from("osh-oxy")))
        .tiebreak(vec![RankCriteria::Index])
        .delimiter(String::from("\x1f"))
        .preview_window(String::from("down:5"))
        .preview(Some(String::new()))
        .multi(false)
        .query(Some(query.to_string()))
        .build()?;

    let (tx_item, rx_item): (SkimItemSender, SkimItemReceiver) = unbounded();

    thread::spawn(move || {
        // TODO merge sort?
        all.sort_by(|a, b| b.partial_cmp(a).unwrap());

        // TODO batch?
        let items: Vec<_> = all.into_iter().map(Arc::new).collect();
        for item in items {
            let _ = tx_item.send(item);
        }

        // notify skim to stop waiting for more
        drop(tx_item);
    });

    let selected_items = Skim::run_with(&options, Some(rx_item))
        .map(|out| out.selected_items)
        .unwrap_or_default();

    let item = selected_items.first().expect("expects one selected item");
    println!("{}", item.text());
    Ok(())
}
