use serde::{Deserialize, Serialize};
use serde_jsonlines::AsyncJsonLinesReader;
use std::{option::Option, path::Path};
use tokio::{fs::File, io::BufReader};
use tokio_stream::StreamExt;

use crate::event::{Event, EventFilter, Events};

/// header of the json lines format.
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct JsonLinesHeader {
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Default for JsonLinesHeader {
    fn default() -> Self {
        Self {
            format: String::from("osh-history-v1"),
            description: None,
        }
    }
}

/// json lines format starts with [`Format`] and then one [`Event`] per line.
#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Entry {
    // have to treat the event as untagged due to untagged Format
    #[serde(rename(deserialize = "event"))]
    EventE { event: Event },
    #[serde(rename(deserialize = "format"))]
    FormatE(JsonLinesHeader),
}

impl Entry {
    /// convert Entry into an Event for filtering
    pub fn maybe_event(self) -> Option<Event> {
        match self {
            Entry::EventE { event } => Some(event),
            Entry::FormatE(_format) => None,
        }
    }
}

pub async fn load_osh_events(
    osh_file: impl AsRef<Path>,
    filter: &EventFilter,
) -> std::io::Result<Events> {
    let fp = BufReader::new(File::open(osh_file).await?);
    let reader = AsyncJsonLinesReader::new(fp);

    Ok(reader
        .read_all::<Entry>()
        .filter_map(|entry_result| match entry_result {
            Ok(entry) => entry.maybe_event().and_then(|e| filter.apply(e)),
            Err(_) => None,
        })
        .collect::<Events>()
        .await)
}
