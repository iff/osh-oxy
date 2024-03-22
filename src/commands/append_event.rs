use anyhow::Context;
use chrono::{TimeZone, Utc};
use osh_oxy::{Entry, Event, Format};
use serde_jsonlines::append_json_lines;

pub(crate) fn invoke(
    starttime: f32,
    command: &str,
    folder: &str,
    endtime: f32,
    exit_code: i16,
    machine: &str,
    session: &str,
) -> anyhow::Result<()> {
    let e = Event {
        timestamp: Utc.timestamp_nanos((starttime * 1e9) as i64).into(),
        command: command.to_string(),
        duration: (endtime - starttime),
        exit_code,
        folder: folder.to_string(),
        machine: machine.to_string(),
        session: session.to_string(),
    };

    // TODO maybe use hostname later, for now use our own file
    let mut home = home::home_dir().expect("home dir has to exist");
    home.push(".osh");
    home.push("active");
    home.push("local_oxy.osh");

    if !home.as_path().exists() {
        // TODO switch to v2 and use tagged?
        let format = Format {
            format: String::from("osh-history-v1"),
            description: None,
        };
        append_json_lines(home.as_path(), [format]).context("failed to serialise header")?;
    }
    append_json_lines(home.as_path(), [Entry::EventE { event: e }])
        .context("failed to serialise event")?;

    Ok(())
}
