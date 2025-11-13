use std::{option::Option, path::Path};

use serde::{Deserialize, Serialize};
use serde_jsonlines::{AsyncJsonLinesReader, AsyncJsonLinesWriter};
use tokio::{
    fs::File,
    io::{AsyncWrite, BufReader},
};
use tokio_stream::StreamExt;

use crate::{
    event::{Event, Events},
    formats::EventWriter,
};

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

pub struct JsonLinesEventWriter<W: AsyncWrite> {
    writer: AsyncJsonLinesWriter<W>,
    header_written: bool,
}

impl<W: AsyncWrite + Unpin> JsonLinesEventWriter<W> {
    pub fn new(writer: W, write_header: bool) -> Self {
        Self {
            writer: AsyncJsonLinesWriter::new(writer),
            header_written: !write_header,
        }
    }
}

impl<W: AsyncWrite + Unpin + Send> EventWriter for JsonLinesEventWriter<W> {
    async fn write(&mut self, event: Event) -> anyhow::Result<()> {
        if !self.header_written {
            self.writer.write(&JsonLinesHeader::default()).await?;
            self.header_written = true;
        }
        self.writer.write(&Entry::EventE { event }).await?;
        Ok(())
    }

    async fn flush(&mut self) -> anyhow::Result<()> {
        self.writer.flush().await?;
        Ok(())
    }
}

pub async fn load_osh_events(osh_file: impl AsRef<Path>) -> std::io::Result<Events> {
    let fp = BufReader::new(File::open(osh_file).await?);
    let reader = AsyncJsonLinesReader::new(fp);

    Ok(reader
        .read_all::<Entry>()
        .filter_map(|entry_result| match entry_result {
            Ok(entry) => entry.maybe_event(),
            Err(_) => None,
        })
        .collect::<Events>()
        .await)
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use super::*;

    #[tokio::test]
    async fn test_parsing_osh_file() -> anyhow::Result<()> {
        let events = load_osh_events(Path::new("tests/local.osh")).await?;
        assert_eq!(events.len(), 5);
        Ok(())
    }
}
