use osh_oxy::*;
use serde_jsonlines::json_lines;
use std::io::Result;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;

fn load_osh(base: &Path) -> Result<Entries> {
    json_lines(base)?.collect::<Result<Entries>>()
}

fn load_simple(base: &mut PathBuf) -> Result<Events> {
    // TODO thread per file
    // events = load_osh(base) + load_zsh(base) + load_legacy(base)

    base.push("local.osh");
    let events = load_osh(base.as_path());

    // TODO prevent clone here? but probably not that heavy..
    let only_events: Result<Events> =
        events.map(|e| e.into_iter().filter_map(|v| v.as_event_or_none()).collect());
    // this is working "in-place" but we end up not knowing that we only have events left
    //events.map(|e| e.retain(|v| v.is_event()));

    // TODO sorting can't be done with map (in-place?)
    match only_events {
        Ok(mut r) => {
            // r.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            r.sort_by(|a, b| b.partial_cmp(&a).unwrap());
            return Ok(r);
        }
        Err(e) => return Err(e),
    };
}

fn main() {
    let mut base = home::home_dir().expect("no home dir found");
    base.push(".osh");

    // using a channel to ship data over
    let (tx, rx) = mpsc::channel();

    let mut fzf = Command::new("fzf")
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

    thread::spawn(move || {
        // TODO batch?
        tx.send(load_simple(&mut base)).unwrap();
    });

    thread::spawn(move || {
        let received = rx.recv().unwrap().unwrap();
        stdin
            .write_all(
                received
                    .into_iter()
                    .map(|e| e.command)
                    .collect::<Vec<String>>()
                    .join("\n")
                    .as_bytes(),
            )
            .expect("Failed to write to stdin");
    });

    let output = fzf.wait_with_output().expect("failed to read stdout");

    // TODO more stable testing
    let mut parts = std::str::from_utf8(&output.stdout)
        .expect("stdout to str")
        .split("\n")
        .collect::<Vec<_>>();
    parts.pop().expect("");
    println!("{}", parts.pop().expect(""));
}

#[cfg(test)]
mod main {
    use super::*;

    #[test]
    fn test_parsing_osh_file() {
        let events = load_simple(Path::new("tests/local.osh"));
        assert!(events.expect("failed").len() == 5);
    }
}
