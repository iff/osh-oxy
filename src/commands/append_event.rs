use anyhow::Context;
use chrono::{TimeZone, Utc};

use crate::{event::Event, formats::json_lines::JsonLinesEventWriter};

pub async fn invoke(
    starttime: f64,
    command: &str,
    folder: &str,
    endtime: f64,
    exit_code: i16,
    machine: &str,
    session: &str,
) -> anyhow::Result<()> {
    let mut osh_file = home::home_dir().context("home dir has to exist")?;
    osh_file.push(".osh/");
    std::fs::create_dir_all(&osh_file)?;
    osh_file.push("local.osh");
    // osh_file.push("local.bosh");

    let event = Event {
        timestamp: Utc.timestamp_nanos((starttime * 1e9) as i64).into(),
        command: command.to_string(),
        duration: (endtime - starttime) as f32,
        exit_code,
        folder: folder.to_string(),
        machine: machine.to_string(),
        session: session.to_string(),
    };

    // TODO maybe write header if file does not exist
    let write_header = !osh_file.as_path().exists();
    let file = tokio::fs::File::open(osh_file.as_path()).await?;
    let mut writer = JsonLinesEventWriter::new(file, write_header);

    // NOTE binary format
    // let mut writer = AsyncBinaryWriter::new(file);

    event.write(&mut writer).await?;

    Ok(())
}
