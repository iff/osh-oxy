use std::{collections::HashSet, thread};

use crate::{
    load_sorted,
    ui::{EventFilter, Tui},
};

/// # Panics
///
/// Panics if loading events fails.
#[expect(clippy::implicit_hasher, reason = "just used in the CLI")]
pub fn invoke(
    query: &str,
    folder: &str,
    session_id: Option<String>,
    filters: HashSet<EventFilter>,
    show_score: bool,
) {
    let (tx_item, receiver) = crossbeam_channel::unbounded();
    thread::spawn(move || {
        // TODO not sure if we want to sort already?
        #[expect(clippy::expect_used, reason = "panic if loading fails")]
        let events = load_sorted().expect("osh files loading");
        for item in events {
            if tx_item.send(item).is_err() {
                break;
            }
        }
    });

    if let Some(event) = Tui::start(receiver, query, folder, session_id, filters, show_score) {
        println!("{}", event.command);
    }
}
