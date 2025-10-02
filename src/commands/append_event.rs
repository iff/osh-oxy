use crate::event::Event;
use crate::json_lines::{Entry, JsonLinesHeader};
use anyhow::Context;
use chrono::{TimeZone, Utc};
use serde_jsonlines::AsyncJsonLinesWriter;

pub(crate) async fn invoke(
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

    // TODO how to write header only for a specific format?
    let mut writer = AsyncJsonLinesWriter::new(tokio::fs::File::open(osh_file.as_path()).await?);
    if !osh_file.as_path().exists() {
        writer
            .write(&JsonLinesHeader::default())
            .await
            .context("serialising json lines header")?;
    }

    let e = Event {
        timestamp: Utc.timestamp_nanos((starttime * 1e9) as i64).into(),
        command: command.to_string(),
        duration: (endtime - starttime) as f32,
        exit_code,
        folder: folder.to_string(),
        machine: machine.to_string(),
        session: session.to_string(),
    };
    writer
        .write(&Entry::EventE { event: e })
        .await
        .context("serialising event")?;

    Ok(())
}
