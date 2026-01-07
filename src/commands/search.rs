use std::{collections::HashSet, sync::Arc, thread};

use crate::{
    load_sorted,
    ui::{EventFilter, Tui},
};

pub fn invoke(
    query: &str,
    folder: &str,
    session_id: Option<String>,
    filters: HashSet<EventFilter>,
    show_score: bool,
) -> anyhow::Result<()> {
    let (tx_item, receiver) = crossbeam_channel::unbounded();
    thread::spawn(|| {
        // TODO not sure if we want to sort already?
        #[allow(clippy::expect_used)]
        let _ = load_sorted()
            .expect("osh files loading")
            .into_iter()
            .map(|item| {
                tx_item
                    .send(Arc::new(item))
                    .expect("sending items through channel");
            })
            .collect::<Vec<_>>();

        drop(tx_item);
    });

    if let Some(event) = Tui::start(receiver, query, folder, session_id, filters, show_score) {
        println!("{}", event.command);
    }
    Ok(())
}
