use std::{collections::HashSet, thread};

use crate::{
    load_sorted,
    ui::{EventFilter, Tui},
};

/// # Errors
///
/// Returns an `Err` if the TUI fails.
#[expect(clippy::implicit_hasher, reason = "just used in the CLI")]
pub fn invoke(
    query: &str,
    folder: &str,
    session_id: Option<String>,
    filters: HashSet<EventFilter>,
    show_score: bool,
) -> anyhow::Result<()> {
    // we use a channel to be able to render the tui instantly
    let (tx_item, receiver) = crossbeam_channel::unbounded();
    thread::spawn(move || {
        // skip silently if loading fails
        if let Ok(events) = load_sorted() {
            for item in events {
                if tx_item.send(item).is_err() {
                    break;
                }
            }
        }
    });

    if let Some(event) = Tui::start(receiver, query, folder, session_id, filters, show_score)? {
        println!("{}", event.command);
    }
    Ok(())
}
