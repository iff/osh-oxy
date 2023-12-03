use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::option::Option;

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
    pub timestamp: DateTime<chrono::Utc>,
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
