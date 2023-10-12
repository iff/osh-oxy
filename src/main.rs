use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_jsonlines::json_lines;
use std::io::Result;
use std::io::Write;
use std::option::Option;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::thread;

// format specs
// {"format": "osh-history-v1", "description": null}
// {"event": {"timestamp": "2023-09-23T06:29:36.257915+00:00", "command": "ll", "duration": 0.009093, "exit-code": 0, "folder": "/home/iff", "machine": "nixos", "session": "93d380e9-4a45-41b1-89e5-447165cf65fc"}}

#[derive(Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd, Clone)]
struct Format {
    format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

//#[derive(Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd, Clone)]
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
struct Event {
    timestamp: DateTime<chrono::Utc>,
    command: String,
    duration: f32,
    exit_code: i16,
    folder: String,
    machine: String,
    session: String,
}

type Events = Vec<Event>;

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
enum Entry {
    // have to treat the event as untagged as well
    #[serde(rename(deserialize = "event"))]
    EventE { event: Event },
    #[serde(rename(deserialize = "format"))]
    FormatE(Format),
}

impl Entry {
    fn as_event_or_none(&self) -> Option<Event> {
        match self {
            Entry::EventE { event } => Some(event.clone()),
            _ => None,
        }
    }

    // fn is_event(&self) -> bool {
    //     match self {
    //         Entry::EventE(_) => true,
    //         _ => false,
    //     }
    // }
}

// we cant really define Ord for Entry
// impl Ord for Entry {
//     fn cmp(&self, other: &Self) -> std::cmp::Ordering {
//         match (self, other) {
//             (Entry::EventE(e1), Entry::EventE(e2)) => e1.timestamp.cmp(&e2.timestamp),
//             (Entry::FormatE(f1), Entry::FormatE(f2)) => f1.format.cmp(&f2.format),
//             (Entry::FormatE(_), Entry::EventE(_)) => std::cmp::Ordering::Less,
//             (Entry::EventE(_), Entry::FormatE(_)) => std::cmp::Ordering::Greater,
//         }
//     }
// }

// impl PartialOrd for Entry {
//     fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
//         match (self, other) {
//             (Entry::FormatE(_), Entry::EventE(_)) => None,
//             (Entry::EventE(_), Entry::FormatE(_)) => None,
//             (Entry::EventE(e1), Entry::EventE(e2)) => Some(e1.cmp(e2)),
//             (Entry::FormatE(f1), Entry::FormatE(f2)) => Some(f1.cmp(f2)),
//         }
//     }
// }
//
// impl PartialEq for Entry {
//     fn eq(&self, other: &Self) -> bool {
//         match (self, other) {
//             (Entry::FormatE(_), Entry::EventE(_)) => false,
//             (Entry::EventE(_), Entry::FormatE(_)) => false,
//             (Entry::EventE(e1), Entry::EventE(e2)) => e1.cmp(e2) == std::cmp::Ordering::Equal,
//             (Entry::FormatE(f1), Entry::FormatE(f2)) => f1.cmp(f2) == std::cmp::Ordering::Equal,
//         }
//     }
// }

type Entries = Vec<Entry>;

fn load_osh(base: &Path) -> Result<Entries> {
    json_lines(base)?.collect::<Result<Entries>>()
}

fn load_simple(base: &Path) -> Result<Events> {
    // # TODO eventually try threads or processes per file? not per file type
    // events = load_osh(base) + load_zsh(base) + load_legacy(base)

    let events = load_osh(base); // todo load rest

    // TODO prevent clone here
    let only_events: Result<Events> =
        events.map(|e| e.into_iter().filter_map(|v| v.as_event_or_none()).collect());
    // this is workin "in-place" but we end up not knowing that we only have events left
    //events.map(|e| e.retain(|v| v.is_event()));

    // TODO merge all vecs before sorting (and returning)
    // events.map(|mut e| e.sort_by(|a, b| a.timestamp.cmp(&b.timestamp)));
    //only_events.map(|mut e| e.sort_by(|a, b| a.cmp(&b)));

    return only_events;
}

fn main() {
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
        let base = Path::new("/home/iff/src/osh-oxy/blackhole.osh");
        tx.send(load_simple(base)).unwrap();
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

    fzf.wait_with_output().expect("failed to read stdout");
}

#[cfg(test)]
mod main {
    use super::*;

    #[test]
    fn test_parsing_osh_file() {
        // TODO format not supported
        let events = load_simple(Path::new("/home/iff/.osh/active/nixos.osh"));
        assert_eq!(events.expect("failed").len(), 450);
    }
}
