use anyhow::Context;
use clap::{Parser, Subcommand};
use osh_oxy::*;
use serde_jsonlines::AsyncJsonLinesReader;
use std::io::Result;
use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use tokio::fs::File;
use tokio::io::BufReader;

async fn load_osh_events(osh_file: impl AsRef<Path>) -> Result<Events> {
    // let events = json_lines(base)?.collect::<Result<Entries>>();
    let fp = BufReader::new(File::open(osh_file).await?);
    let reader = AsyncJsonLinesReader::new(fp);
    let events = reader
        .read_all::<Event>()
        .collect::<std::io::Result<Vec<_>>>()
        .await?;
    events.map(|e| e.into_iter().filter_map(|v| v.as_event_or_none()).collect())
}

// fn load_simple(base: impl AsMut<Path>) -> Result<Events> {
//     // TODO thread per file
//     // events = load_osh(base) + load_zsh(base) + load_legacy(base)
//
//     base.push("local.osh");
//     let events = load_osh_events(base);
//
//     // maybe prevent clone here? but probably not that heavy..
//     let only_events: Result<Events> =
//         events.map(|e| e.into_iter().filter_map(|v| v.as_event_or_none()).collect());
//     // this is working "in-place" but we end up not knowing that we only have events left
//     //events.map(|e| e.retain(|v| v.is_event()));
//
//     // TODO sorting can't be done with map (in-place?)
//     match only_events {
//         Ok(mut r) => {
//             // r.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
//             r.sort_by(|a, b| b.partial_cmp(&a).unwrap());
//             return Ok(r);
//         }
//         Err(e) => return Err(e),
//     };
// }

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
            let mut base = home::home_dir().expect("no home dir found");
            base.push(".osh");

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

            // TODO load all files
            base.push("local.osh");
            let events = load_osh_events(base).await;

            thread::spawn(move || {
                // TODO batch?
                match events {
                    Ok(mut r) => {
                        // r.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                        r.sort_by(|a, b| b.partial_cmp(&a).unwrap());
                        tx.send(r);
                    }
                    Err(e) => {},
                }
                // tx.send(load_simple(&mut base)).unwrap();
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
