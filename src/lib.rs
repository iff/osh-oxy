use chrono::{DateTime, Local};
use glob::glob;
use serde::{Deserialize, Serialize};
use serde_jsonlines::AsyncJsonLinesReader;
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

pub type Entries = Vec<Entry>;

pub async fn load_osh_events(osh_file: impl AsRef<Path>) -> std::io::Result<Events> {
    let fp = BufReader::new(File::open(osh_file).await?);
    let reader = AsyncJsonLinesReader::new(fp);
    let events = reader
        .read_all::<Entry>()
        .collect::<std::io::Result<Vec<_>>>()
        .await;

    events.map(|e| {
        e.into_iter()
            .filter_map(|v| v.as_event_or_none())
            .collect::<Events>()
    })
}

pub fn osh_files() -> Vec<PathBuf> {
    let home = home::home_dir().expect("no home dir found");
    let pattern = format!("{}/.osh/*/*.osh", home.to_str().expect(""));
    glob(&pattern)
        .expect("failed to read glob pattern")
        .filter_map(Result::ok)
        .collect()
}

#[cfg(test)]
mod serach {
    use super::*;
    use std::path::Path;

    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    #[test]
    fn test_parsing_osh_file() {
        let events = aw!(load_osh_events(Path::new("tests/local.osh")));
        assert!(events.expect("failed").len() == 5);
    }
}
