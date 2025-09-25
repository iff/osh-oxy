use crate::event::{Event, EventFilter, load_osh_events, osh_files};
use chrono::Utc;
use futures::future;
use itertools::kmerge_by;
use skim::{
    ItemPreview, PreviewContext, RankCriteria, Skim, SkimItem, SkimItemReceiver, SkimItemSender,
    prelude::{Key, SkimOptionsBuilder, unbounded},
};
use std::borrow::Cow;
use std::{sync::Arc, thread};

impl SkimItem for Event {
    fn text(&self) -> Cow<'_, str> {
        let f = timeago::Formatter::new();
        let ago = f.convert_chrono(self.timestamp, Utc::now());
        Cow::Owned(format!("{ago} --- {}", self.command))
    }

    fn output(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.command)
    }

    fn preview(&self, _context: PreviewContext) -> ItemPreview {
        let f = timeago::Formatter::new();
        let ago = f.convert_chrono(self.timestamp, Utc::now());
        ItemPreview::Text(format!(
            "[{}] [exit_code={}]\n{}",
            ago, self.exit_code, self.command
        ))
    }
}

pub(crate) async fn invoke(query: &str, session_id: Option<String>) -> anyhow::Result<()> {
    let oshs = osh_files();
    // TODO filter here and in parallel?
    let filter = EventFilter::new(session_id);
    let all = future::try_join_all(oshs.into_iter().map(|f| load_osh_events(f, &filter))).await?;

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
        let iterators = all.into_iter().map(|ev| ev.into_iter().rev());
        for item in kmerge_by(iterators, |a: &Event, b: &Event| a > b) {
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
