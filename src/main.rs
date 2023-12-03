use anyhow::Context;
use clap::{Parser, Subcommand};
use futures::future;
use glob::glob;
use osh_oxy::*;
use serde_jsonlines::AsyncJsonLinesReader;
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
    events.map(|e| e.into_iter().filter_map(|v| v.as_event_or_none()).collect())
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
#[clap(rename_all = "snake_case")]
enum Command {
    Search {},
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Search {} => {
            // using a channel to ship data over
            let (tx, rx) = mpsc::channel();

            let mut fzf = std::process::Command::new("fzf")
                .arg("--height=70%")
                .arg("--min-height=10")
                .arg("--header=some-header")
                .arg("--tiebreak=index")
                //.arg("--read0")
                .arg("--delimiter=\x1f")
                .arg("--preview-window=down:10:wrap")
                //.arg("--print0")
                .arg("--print-query")
                .arg("--expect=enter")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("failed to spawn child process");

            let mut stdin = fzf.stdin.take().expect("failed to open stdin");

            let home = home::home_dir().expect("no home dir found");
            let oshs = glob(format!("{}/.osh/**/*.osh", home.display()).as_str())?;

            let mut all = future::try_join_all(oshs.map(|p| load_osh_events(p.expect(""))))
                .await?
                .into_iter()
                .flatten()
                .collect::<Vec<Event>>();

            thread::spawn(move || {
                // TODO batch?
                all.sort_by(|a, b| b.partial_cmp(&a).unwrap());
                let _ = tx.send(all);
            });

            thread::spawn(move || {
                let received = rx.recv().unwrap();
                stdin
                    .write_all(
                        received
                            .into_iter()
                            .map(|e| e.command) // TODO: display more
                            .collect::<Vec<String>>()
                            .join("\n")
                            .as_bytes(),
                    )
                    .expect("failed to write to stdin");
            });

            let output = fzf.wait_with_output().expect("failed to read stdout");

            // TODO more stable testing
            let mut parts = std::str::from_utf8(&output.stdout)
                .expect("stdout to str")
                .split("\n")
                .collect::<Vec<_>>();
            parts.pop().expect("expects one item");
            println!("{}", parts.pop().expect("expects one item"));
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
