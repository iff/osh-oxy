use chrono::Utc;
use futures::future;
use osh_oxy::{load_osh_events, osh_files, Event, EventFilter, Events};
use std::io::Write;
use std::sync::mpsc;
use std::thread;

pub(crate) async fn invoke(
    query: &str,
    session_id: Option<String>,
    session_start: Option<f32>,
) -> anyhow::Result<()> {
    let filter = EventFilter::new(session_id, session_start);
    let (tx, rx) = mpsc::channel();

    // needs sh to be able to use echo in preview
    // TODO: --read0 --print0
    // tty? or just produce output and pipe?
    let mut fzf = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("fzf --height=70% --min-height=10 --header=osh-oxy --tiebreak=index --delimiter=\x1f --preview-window=down:2:wrap --with-nth=1 --preview=\"printf '[%s] [%s]\\n%s' \"{{2}}\" \"{{3}}\" \"{{4}}\"\" --print-query --expect=enter --query={}", query))
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("failed to spawn child process");

    let mut stdin = fzf.stdin.take().expect("failed to open stdin");

    // TODO maybe we don't need the join here?
    let oshs = osh_files();
    let mut all = future::try_join_all(oshs.into_iter().map(|f| load_osh_events(f, &filter)))
        .await?
        .into_iter()
        .flatten()
        .collect::<Events>();

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

    Ok(())
}
