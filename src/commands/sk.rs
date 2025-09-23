use futures::future;
use osh_oxy::{load_osh_events, osh_files, EventFilter, Events};
use skim::{
    prelude::{unbounded, Key, SkimOptionsBuilder},
    RankCriteria, Skim, SkimItemReceiver, SkimItemSender,
};
use std::{sync::Arc, thread};

pub(crate) async fn invoke(query: &str, session_id: Option<String>) -> anyhow::Result<()> {
    let oshs = osh_files();
    // TODO filter here and in parallel?
    let filter = EventFilter::new(session_id);
    let mut all = future::try_join_all(oshs.into_iter().map(|f| load_osh_events(f, &filter)))
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
        .bind(vec![
            String::from("Enter:accept"),
            String::from("esc:abort"),
            String::from("ctrl-c:abort"),
        ])
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
