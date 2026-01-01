use std::{io::Write, path::PathBuf};

use arbitrary::Arbitrary;
use serde::{Deserialize, Serialize};

#[allow(deprecated)]
use crate::formats::json_lines::JsonLineEvent;
use crate::formats::rmp::BinaryWriter;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Event {
    pub timestamp_millis: i64,
    pub command: String,
    pub duration: f32,
    pub exit_code: i16,
    pub folder: String,
    pub machine: String,
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
            duration: u.arbitrary()?,
            exit_code: u.arbitrary()?,
            folder: folder.to_string_lossy().into(),
            machine: machine_id,
            session: session_id,
        })
    }
}

#[allow(deprecated)]
impl From<JsonLineEvent> for Event {
    fn from(event: JsonLineEvent) -> Self {
        let timestamp = event.timestamp.timestamp_millis();
        Self {
            timestamp_millis: timestamp,
            command: event.command,
            duration: event.duration,
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
        self.endtimestamp().cmp(&other.endtimestamp())
    }
}

impl Event {
    pub fn write<W: Write>(self, writer: &mut BinaryWriter<W>) -> anyhow::Result<()> {
        writer.write(self)
    }

    pub fn endtimestamp(&self) -> i64 {
        self.timestamp_millis + ((self.duration * 1000.0) as i64)
    }
}
