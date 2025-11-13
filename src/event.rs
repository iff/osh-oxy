use std::{option::Option, path::PathBuf};

use arbitrary::{Arbitrary, Result, Unstructured};
use chrono::{DateTime, Local, TimeZone, Utc};
use serde::{Deserialize, Serialize};

// use skim::{AnsiString, DisplayContext, ItemPreview, PreviewContext, SkimItem};
use crate::formats::EventWriter;

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
    pub async fn write(self, writer: &mut impl EventWriter) -> anyhow::Result<()> {
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

pub type Events = Vec<Event>;
