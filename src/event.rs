use arbitrary::{Arbitrary, Result, Unstructured};
use chrono::{DateTime, Local, TimeZone, Utc};
use glob::glob;
use serde::{Deserialize, Serialize};
use serde_jsonlines::AsyncJsonLinesReader;
use std::collections::HashSet;
use std::option::Option;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::BufReader;
use tokio_stream::StreamExt;

// {"format": "osh-history-v1", "description": null}
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Format {
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub struct Event {
    pub timestamp: DateTime<Local>,
    pub command: String,
    pub duration: f32,
    pub exit_code: i16,
    pub folder: String,
    pub machine: String,
    pub session: String,
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

pub type Events = Vec<Event>;

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Entry {
    // have to treat the event as untagged due to untagged Format
    #[serde(rename(deserialize = "event"))]
    EventE { event: Event },
    #[serde(rename(deserialize = "format"))]
    FormatE(Format),
}

impl Entry {
    pub fn as_event_or_none(&self) -> Option<Event> {
        match self {
            Entry::EventE { event } => Some(event.clone()),
            _ => None,
        }
    }

    // fn is_event(&self) -> bool {
    //     match self {
    //         Entry::EventE{_} => true,
    //         _ => false,
    //     }
    // }
}

// pub type Entries = Vec<Entry>;

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

pub async fn load_osh_events(
    osh_file: impl AsRef<Path>,
    filter: &EventFilter,
) -> std::io::Result<Events> {
    let fp = BufReader::new(File::open(osh_file).await?);
    let reader = AsyncJsonLinesReader::new(fp);
    let events = reader
        .read_all::<Entry>()
        .collect::<std::io::Result<Vec<_>>>()
        .await;

    events.map(|e| {
        e.into_iter()
            .filter_map(|v| v.as_event_or_none())
            .filter_map(|v| filter.apply(v))
            .collect::<Events>()
    })
}

pub fn osh_files() -> HashSet<PathBuf> {
    let home = home::home_dir().expect("no home dir found");
    let pattern = format!("{}/.osh/**/*.osh", home.to_str().expect(""));

    glob(&pattern)
        .expect("failed to read glob pattern")
        .filter_map(Result::ok)
        .filter_map(|path| path.canonicalize().ok())
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::Path;

    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    #[test]
    fn test_parsing_osh_file() {
        let filter = EventFilter::new(None);
        let events = aw!(load_osh_events(Path::new("tests/local.osh"), &filter)).unwrap();
        assert_eq!(events.len(), 5);
    }

    #[test]
    fn test_filter_session_id() {
        let filter = EventFilter::new(Some(String::from("5ed2cbda-4821-4f00-8a67-468aaa301377")));
        let events = aw!(load_osh_events(Path::new("tests/local.osh"), &filter)).unwrap();
        assert_eq!(events.len(), 2);
    }
}
