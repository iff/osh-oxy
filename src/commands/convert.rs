use anyhow::Context;
use tokio::fs::File;

use crate::{
    event::EventFilter,
    formats::{EventWriter, Kind, json_lines, rmp::AsyncBinaryWriter},
    osh_files,
};

pub async fn invoke() -> anyhow::Result<()> {
    for path in osh_files(Kind::JsonLines) {
        let filter = EventFilter::new(None);
        let events = json_lines::load_osh_events(path.clone(), &filter)
            .await
            .context("Failed to load events from JSON lines file")?;

        let output_path = path.with_extension("bosh");

        let file = File::create(&output_path)
            .await
            .context("Failed to create output file")?;
        let mut writer = AsyncBinaryWriter::new(file);
        for event in events {
            writer
                .write(event)
                .await
                .context("Failed to write event")?;
        }
    }

    Ok(())
}
