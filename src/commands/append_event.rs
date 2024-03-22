use anyhow::Context;
use chrono::{TimeZone, Utc};
use osh_oxy::{Entry, Event, Format};
use serde_jsonlines::append_json_lines;

pub(crate) fn invoke(
    starttime: f64,
    command: &str,
    folder: &str,
    endtime: f64,
    exit_code: i16,
    machine: &str,
    session: &str,
) -> anyhow::Result<()> {
    // TODO maybe use hostname later, for now use our own file
    let mut osh_file = home::home_dir().expect("home dir has to exist");
    osh_file.push(".osh/active/local_oxy.osh");

    if !osh_file.as_path().exists() {
        // TODO default header?
        append_json_lines(
            osh_file.as_path(),
            [Format {
                format: String::from("osh-history-v1"),
                description: None,
            }],
        )
        .context("failed to serialise header")?;
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
    append_json_lines(osh_file.as_path(), [Entry::EventE { event: e }])
        .context("failed to serialise event")?;

    Ok(())
}
