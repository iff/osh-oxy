use crate::event::EventFilter;
use crate::formats::json_lines;
use crate::formats::rmp::AsyncBinaryWriter;
use anyhow::Context;
use std::path::Path;
use tokio::fs::File;

pub async fn invoke(path: &String) -> anyhow::Result<()> {
    let path = Path::new(path);
    let filter = EventFilter::new(None);
    let events = json_lines::load_osh_events(path, &filter)
        .await
        .context("Failed to load events from JSON lines file")?;

    let output_path = path.with_extension("bosh");

    let file = File::create(&output_path)
        .await
        .context("Failed to create output file")?;
    let mut writer = AsyncBinaryWriter::new(file);
    for event in events {
        writer
            .write(&event)
            .await
            .context("Failed to write event")?;
    }

    Ok(())
}
