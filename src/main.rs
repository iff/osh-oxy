use serde::{Deserialize, Serialize};
use serde_jsonlines::json_lines;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::thread;

// {"format": "osh-history-v1", "description": null}
// {"event": {"timestamp": "2023-09-23T06:29:36.257915+00:00", "command": "ll", "duration": 0.009093, "exit-code": 0, "folder": "/home/iff", "machine": "nixos", "session": "93d380e9-4a45-41b1-89e5-447165cf65fc"}}

#[derive(Serialize, Deserialize)]
struct Format {
    format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Event {
    timestamp: datetime,
    command: String,
    duration: f32,
    exit_code: i8,
    folder: String,
    machine: String,
    session: String,
}

#[derive(Serialize, Deserialize)]
enum Entry {
    Event { event: Event },
    Format { format: Format },
}

fn load_osh(base: &Path) -> Result<Vec<Entry>, Err> {
    json_lines(base)?.collect::<Result<Vec<Entry>>>()?
}

fn load_simple(base: &Path) -> Vec<Event> {
    // # TODO eventually try threads or processes per file? not per file type
    // events = load_osh(base) + load_zsh(base) + load_legacy(base)

    let events = load_osh(base); // todo load rest
    events.sort_by(|a, b| a.timestamp.partial_cmp(b.timestamp));
    return events;
}

fn main() {
    // using a channel to ship data over
    let (tx, rx) = mpsc::channel();

    let events = thread::spawn(move || {
        let base = Path::new(".osh/bar.txt");
        tx.send(load_simple(base)).unwrap();
    });

    let fzf = thread::spawn(move || {
        let received = rx.recv().unwrap();

        let fzf = Command::new("fzf")
            .arg("--height=70%")
            .arg("--min-height=10")
            .arg("--header=some-header")
            .arg("--tiebreak=index")
            .arg("--read0")
            .arg("--delimiter=\x1f")
            .arg("--preview-window=down:10:wrap")
            .arg("--preview=python -m draft get-preview {1}")
            .arg("--print0")
            .arg("--print-query")
            .arg("--expect=enter");

        // start fzf and wait for input on stdin?
    });
}
