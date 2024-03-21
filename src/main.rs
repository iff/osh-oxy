use crate::Entry::EventE;
use anyhow::Context;
use chrono::TimeZone;
use chrono::Utc;
use clap::{Parser, Subcommand};
use futures::future;
use glob::glob;
use osh_oxy::*;
use serde_jsonlines::{append_json_lines, AsyncJsonLinesReader};
use std::io::Result;
use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use tokio::fs::File;
use tokio::io::BufReader;
use tokio_stream::StreamExt;

async fn load_osh_events(osh_file: impl AsRef<Path>) -> Result<Events> {
    let fp = BufReader::new(File::open(osh_file).await?);
    let reader = AsyncJsonLinesReader::new(fp);
    let events = reader
        .read_all::<Entry>()
        .collect::<std::io::Result<Vec<_>>>()
        .await;

    events.map(|e| {
        e.into_iter()
            .filter_map(|v| v.as_event_or_none())
            .collect::<Events>()
    })
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
#[clap(rename_all = "kebab_case")]
enum Command {
    Search {
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        session_start: Option<f32>,
    },
    AppendEvent {
        #[arg(long)]
        starttime: f32,
        #[arg(long)]
        command: String,
        #[arg(long)]
        folder: String,
        #[arg(long)]
        endtime: f32,
        #[arg(long)]
        exit_code: i16,
        #[arg(long)]
        machine: String,
        #[arg(long)]
        session: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::AppendEvent {
            starttime,
            command,
            folder,
            endtime,
            exit_code,
            machine,
            session,
        } => {
            let e = Event {
                timestamp: Utc.timestamp_nanos((starttime * 1e9) as i64).into(),
                command,
                duration: (endtime - starttime),
                exit_code,
                folder,
                machine,
                session,
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
                append_json_lines(home.as_path(), [format])
                    .context("failed to serialise header")?;
            }
            append_json_lines(home.as_path(), [EventE { event: e }])
                .context("failed to serialise event")?;
        }
        Command::Search {
            query: _,
            session_id: _,
            session_start: _,
        } => {
            let (tx, rx) = mpsc::channel();

            // if session_start is not None:
            //     session_start = datetime.fromtimestamp(session_start, tz=timezone.utc)

            // needs sh to be able to use echo in preview
            // TODO: --read0 --print0
            // tty? or just produce output and pipe?
            let mut fzf = std::process::Command::new("sh")
                .arg("-c")
                // FIXME previewing {4} somhow executes the command?
                .arg("fzf --height=70% --min-height=10 --header=osh-oxy --tiebreak=index --delimiter=\x1f --preview-window=down:10:wrap --with-nth=1 --preview=\"print -a \\[{2}\\] \\[{3}\\]\" --print-query --expect=enter")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("failed to spawn child process");

            let mut stdin = fzf.stdin.take().expect("failed to open stdin");

            let home = home::home_dir().expect("no home dir found");
            let oshs = glob((home.to_str().expect("").to_owned() + "/.osh/*/*.osh").as_str())?;

            // TODO maybe we don't need the join here?
            let mut all = future::try_join_all(oshs.map(|p| load_osh_events(p.expect(""))))
                .await?
                .into_iter()
                .flatten()
                .collect::<Vec<Event>>();

            thread::spawn(move || {
                // TODO merge sort?
                all.sort_by(|a, b| b.partial_cmp(a).unwrap());
                // TODO batch?
                let _ = tx.send(all);
            });

            thread::spawn(move || {
                let received = rx.recv().unwrap();

                let fmt = |e: Event| -> String {
                    // TODO can we make this shorter, eg s/minutes/m?
                    let f = timeago::Formatter::new();
                    let ago = f.convert_chrono(e.timestamp, Utc::now());
                    format!(
                        "{:>15} --- {}\x1f{}\x1fexit_code={}\x1f{}",
                        ago, e.command, ago, e.exit_code, e.command
                    )
                };

                stdin
                    .write_all(
                        received
                            .into_iter()
                            .map(fmt)
                            .collect::<Vec<String>>()
                            .join("\n")
                            .as_bytes(),
                    )
                    .expect("failed to write to stdin");
            });

            let output = fzf.wait_with_output().expect("failed to read stdout");

            // TODO handle output.status (and output.stderr)
            // if !output.status.success() {
            //     let err = std::str::from_utf8(&output.stderr).expect("stderr");
            //     panic!(
            //         "exited with {}: {}",
            //         output.status.code().ok_or(-1 as i32).unwrap(),
            //         err
            //     );
            // }

            // TODO this is shaky
            let mut parts = std::str::from_utf8(&output.stdout)
                .expect("stdout to str")
                .split('\n')
                .collect::<Vec<_>>();
            parts.pop().expect("expects one item");
            let command = parts.pop().expect("expects one item");
            let command_parts = command.split('\x1f').collect::<Vec<_>>();
            println!(
                "{}",
                command_parts.last().expect("expect last to be command")
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod main {
    use super::*;

    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    #[test]
    fn test_parsing_osh_file() {
        let events = aw!(load_osh_events(Path::new("tests/local.osh")));
        assert!(events.expect("failed").len() == 5);
    }
}
