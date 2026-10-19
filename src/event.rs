use std::{io::Write, path::PathBuf, time::Duration};

use arbitrary::Arbitrary;
use serde::{Deserialize, Serialize};

#[expect(deprecated, reason = "migration to binary format")]
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

#[expect(deprecated, reason = "migration to binary format")]
impl From<JsonLineEvent> for Event {
    /// Converts a `JsonLineEvent` to the new binary format. The `JsonLineEven` format is deprecated
    /// and this is only used to convert old history files to the binary format.
    fn from(event: JsonLineEvent) -> Self {
        let timestamp = event.timestamp.timestamp_millis();
        #[expect(clippy::cast_possible_truncation, reason = "duration is small")]
        let endtime = timestamp + (event.duration * 1000.).round() as i64;
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

impl Event {
    /// # Errors
    ///
    /// Will return an `Err` if serialisation or writing fails.
    pub fn write<W: Write>(self, writer: &mut BinaryWriter<W>) -> anyhow::Result<()> {
        writer.write(self)
    }

    /// time elapsed since the event ended, relative to `now_millis`
    #[must_use]
    pub fn elapsed_since_end(&self, now_millis: i64) -> Duration {
        // future endtimes (clock skew between machines) count as now
        let elapsed = u64::try_from(now_millis.saturating_sub(self.endtime)).unwrap_or(0);
        Duration::from_millis(elapsed)
    }
}

#[cfg(test)]
mod tests {
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
    fn elapsed_since_end_in_past() {
        let event = event_with_endtime(1_000);
        assert_eq!(event.elapsed_since_end(3_500), Duration::from_millis(2_500));
    }

    #[test]
    fn elapsed_since_end_in_future_is_zero() {
        let event = event_with_endtime(5_000);
        assert_eq!(event.elapsed_since_end(1_000), Duration::ZERO);
    }

    #[test]
    fn elapsed_since_end_saturates() {
        let event = event_with_endtime(i64::MIN);
        assert_eq!(
            event.elapsed_since_end(1_000),
            Duration::from_millis(i64::MAX.cast_unsigned())
        );
    }

    #[expect(deprecated, reason = "migration to binary format")]
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
}
