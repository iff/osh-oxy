use std::fs::File;

use anyhow::Context;

use crate::{
    event::Event,
    formats::{Kind, json_lines, rmp::BinaryWriter},
    mmap, osh_files,
};

/// convert from json lins to rmp
pub fn invoke() -> anyhow::Result<()> {
    for path in osh_files(Kind::JsonLines)? {
        let file = File::open(&path)?;
        let data = mmap(&file);
        let events = json_lines::load_osh_events(data)
            .context("Failed to load events from JSON lines file")?;

        let output_path = path.with_extension("bosh");

        let file = std::fs::File::create(&output_path).context("Failed to create output file")?;
        let mut writer = BinaryWriter::new(file);
        for event in events {
            Event::from(event)
                .write(&mut writer)
                .context("Failed to write event")?;
        }
    }

    Ok(())
}
