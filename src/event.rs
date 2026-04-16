use std::{io::Write, path::PathBuf};

use arbitrary::Arbitrary;
use serde::{Deserialize, Serialize};

#[allow(deprecated)]
use crate::formats::json_lines::JsonLineEvent;
use crate::formats::rmp::BinaryWriter;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Event format for entries in the history file.
pub struct Event {
    /// time when execution of the command began
    pub timestamp_millis: i64,
    pub command: String,
    /// records time when the command ended (can be used to calculate duration)
    pub endtime: i64,
    pub exit_code: i16,
    pub folder: String,
    /// a special machine id to filter by machine
    pub machine: String,
    /// a special session id to filter by session
    pub session: String,
}

impl Arbitrary<'_> for Event {
    fn arbitrary(u: &mut arbitrary::Unstructured) -> arbitrary::Result<Self> {
        let folder: PathBuf = u.arbitrary()?;
        let machine_id: String = u.arbitrary()?;
        let session_id: String = u.arbitrary()?;
        Ok(Event {
            timestamp_millis: u.arbitrary()?,
            command: u.arbitrary()?,
            endtime: u.arbitrary()?,
            exit_code: u.arbitrary()?,
            folder: folder.to_string_lossy().into(),
            machine: machine_id,
            session: session_id,
        })
    }
}

#[allow(deprecated)]
impl From<JsonLineEvent> for Event {
    /// Converts a `JsonLineEvent` to the new binary format. The `JsonLineEven` format is deprecated
    /// and this is only used to convert old history files to the binary format.
    fn from(event: JsonLineEvent) -> Self {
        let timestamp = event.timestamp.timestamp_millis();
        let endtime = timestamp + (event.duration * 1000.) as i64;
        Self {
            timestamp_millis: timestamp,
            command: event.command,
            endtime,
            exit_code: event.exit_code,
            folder: event.folder,
            machine: event.machine,
            session: event.session,
        }
    }
}

impl Eq for Event {}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Event {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.endtime.cmp(&other.endtime)
    }
}

impl Event {
    pub fn write<W: Write>(self, writer: &mut BinaryWriter<W>) -> anyhow::Result<()> {
        writer.write(self)
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::*;

    fn event_with_endtime(endtime: i64) -> Event {
        Event {
            timestamp_millis: 0,
            command: String::new(),
            endtime,
            exit_code: 0,
            folder: String::new(),
            machine: String::new(),
            session: String::new(),
        }
    }

    #[test]
    fn cmp_orders_by_endtime() {
        let earlier = event_with_endtime(100);
        let later = event_with_endtime(200);
        assert_eq!(earlier.cmp(&later), Ordering::Less);
        assert_eq!(later.cmp(&earlier), Ordering::Greater);
        assert_eq!(earlier.cmp(&earlier), Ordering::Equal);
    }

    #[test]
    fn partial_cmp_always_some() {
        let a = event_with_endtime(100);
        let b = event_with_endtime(200);
        assert_eq!(a.partial_cmp(&b), Some(Ordering::Less));
        assert_eq!(b.partial_cmp(&a), Some(Ordering::Greater));
        assert_eq!(a.partial_cmp(&a), Some(Ordering::Equal));
    }

    #[allow(deprecated)]
    #[test]
    fn from_json_line_event() {
        let timestamp = chrono::DateTime::from_timestamp_millis(1_000_000_000_000).unwrap();
        let timestamp = timestamp.with_timezone(&chrono::Local);
        let json_event = crate::formats::json_lines::JsonLineEvent {
            timestamp,
            command: "sleep 5".to_string(),
            duration: 5.0,
            exit_code: 0,
            folder: "/".to_string(),
            machine: "m".to_string(),
            session: "s".to_string(),
        };
        let event = Event::from(json_event);
        assert_eq!(event.timestamp_millis, 1_000_000_000_000);
        assert_eq!(event.endtime, 1_000_000_000_000_i64 + 5 * 1000);
        assert_eq!(event.command, "sleep 5");
        assert_eq!(event.exit_code, 0);
        assert_eq!(event.folder, "/");
        assert_eq!(event.machine, "m");
        assert_eq!(event.session, "s");
    }

    #[test]
    fn sort_by_endtime() {
        let mut events = vec![
            event_with_endtime(300),
            event_with_endtime(100),
            event_with_endtime(200),
        ];
        events.sort();
        assert_eq!(
            events.iter().map(|e| e.endtime).collect::<Vec<_>>(),
            vec![100, 200, 300]
        );
    }
}
