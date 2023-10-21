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

// {"event": {"timestamp": "2023-09-23T06:29:36.257915+00:00", "command": "ll", "duration": 0.009093, "exit-code": 0, "folder": "/home/iff", "machine": "nixos", "session": "93d380e9-4a45-41b1-89e5-447165cf65fc"}}
//#[derive(Eq, PartialEq, Ord, PartialOrd)]
#[derive(Serialize, Deserialize, Clone)]
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
    // have to treat the event as untagged as well
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
