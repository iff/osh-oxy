#![allow(deprecated)]

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// the metadata we store for each history entry
#[deprecated(since = "0.2.0", note = "use binary format")]
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct JsonLineEvent {
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

impl PartialOrd for JsonLineEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.endtimestamp().cmp(&other.endtimestamp()))
    }
}

impl JsonLineEvent {
    pub fn endtimestamp(&self) -> i64 {
        self.timestamp.timestamp_millis() + ((self.duration * 1000.0) as i64)
    }
}

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

/// json lines format starts with [`JsonLinesHeader`] and then one [`Event`] per line.
#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Entry {
    // have to treat the event as untagged due to untagged Format
    #[serde(rename(deserialize = "event"))]
    EventE { event: JsonLineEvent },
    #[serde(rename(deserialize = "format"))]
    FormatE(JsonLinesHeader),
}

impl Entry {
    /// convert Entry into an Event for filtering
    pub fn maybe_event(self) -> Option<JsonLineEvent> {
        match self {
            Entry::EventE { event } => Some(event),
            Entry::FormatE(_format) => None,
        }
    }
}

pub fn load_osh_events(data: &[u8]) -> std::io::Result<Vec<JsonLineEvent>> {
    let mut events = Vec::new();
    for line in data.split(|c| *c == b'\n') {
        let line = unsafe { std::str::from_utf8_unchecked(line) };
        if let Ok(entry) = serde_json::from_str::<Entry>(line)
            && let Some(event) = entry.maybe_event()
        {
            events.push(event);
        }
    }

    Ok(events)
}

#[cfg(test)]
mod test {
    use std::{fs::File, path::Path};

    use super::*;
    use crate::mmap;

    #[test]
    fn test_parsing_osh_file() -> anyhow::Result<()> {
        let path = Path::new("tests/local.osh");
        let file = File::open(path).unwrap();
        let data = mmap(&file);
        let events = load_osh_events(data)?;
        assert_eq!(events.len(), 5);
        Ok(())
    }
}
