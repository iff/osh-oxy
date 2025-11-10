use crate::formats::EventWriter;
use arbitrary::{Arbitrary, Result, Unstructured};
use chrono::{DateTime, Local, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use skim::{AnsiString, DisplayContext, ItemPreview, PreviewContext, SkimItem};
use std::borrow::Cow;
use std::option::Option;
use std::path::PathBuf;

/// the metadata we store for each history entry
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Event {
    /// start time of the event
    pub timestamp: DateTime<Local>,
    pub command: String,
    /// duration in seconds with fractional nanoseconds (on linux)
    pub duration: f32,
    pub exit_code: i16,
    pub folder: String,
    pub machine: String,
    pub session: String,
}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.endtime().cmp(&other.endtime()))
    }
}

impl Event {
    pub async fn write(&self, writer: &mut impl EventWriter) -> anyhow::Result<()> {
        writer.write(self).await
    }
    pub fn endtime(&self) -> DateTime<Local> {
        self.timestamp + chrono::Duration::milliseconds((self.duration * 1000.0) as i64)
    }
}

impl<'a> Arbitrary<'a> for Event {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        // TOOD bounds?
        let i: i64 = u.arbitrary()?;
        let folder: PathBuf = u.arbitrary()?;
        let machine_id: String = u.arbitrary()?;
        let session_id: String = u.arbitrary()?;
        Ok(Event {
            timestamp: Utc.timestamp_nanos(i * 1_000_000_000).into(),
            command: u.arbitrary()?,
            duration: u.arbitrary()?,
            exit_code: u.arbitrary()?,
            folder: folder.to_string_lossy().to_string(),
            machine: format!("machine_{machine_id}"),
            session: format!("session_{session_id}"),
        })
    }
}

impl SkimItem for Event {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.command)
    }

    fn display<'a>(&'a self, _context: DisplayContext<'a>) -> AnsiString<'a> {
        // TODO context?
        let f = timeago::Formatter::new();
        let ago = f.convert_chrono(self.timestamp, Utc::now());
        AnsiString::new_string(format!("{ago} --- {}", self.command), Vec::new())
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

pub type Events = Vec<Event>;

// TODO maybe later something more generic
pub struct EventFilter {
    session_id: Option<String>,
}

impl EventFilter {
    pub fn new(session_id: Option<String>) -> Self {
        Self { session_id }
    }

    pub fn apply(&self, event: Event) -> Option<Event> {
        match &self.session_id {
            None => {}
            Some(session_id) => {
                if event.session != *session_id {
                    return None;
                }
            }
        }

        Some(event)
    }
}

#[cfg(test)]
mod test {
    use crate::formats::json_lines;

    use super::*;
    use std::path::Path;

    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    #[test]
    fn test_filter_session_id() {
        let filter = EventFilter::new(Some(String::from("5ed2cbda-4821-4f00-8a67-468aaa301377")));
        let events = aw!(json_lines::load_osh_events(
            Path::new("tests/local.osh"),
            &filter
        ))
        .unwrap();
        assert_eq!(events.len(), 2);
    }
}
