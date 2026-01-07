use clap::{Parser, Subcommand};
use osh_oxy::{commands, ui::EventFilter};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
#[clap(rename_all = "kebab_case")]
enum Command {
    AppendEvent {
        #[arg(long)]
        starttime: i64,
        #[arg(long)]
        command: String,
        #[arg(long)]
        folder: String,
        #[arg(long)]
        endtime: f64,
        #[arg(long)]
        exit_code: i16,
        #[arg(long)]
        machine: String,
        #[arg(long)]
        session: String,
    },
    Cat {},
    Convert {},
    Search {
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        folder: String,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        filter: Option<EventFilter>,
        #[arg(long)]
        show_score: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Cat {} => commands::cat::invoke()?,
        Command::AppendEvent {
            starttime,
            command,
            folder,
            endtime,
            exit_code,
            machine,
            session,
        } => commands::append_event::invoke(
            starttime, &command, &folder, endtime, exit_code, &machine, &session,
        )?,
        Command::Convert {} => commands::convert::invoke()?,
        Command::Search {
            query,
            folder,
            session_id,
            filter,
            show_score,
        } => commands::search::invoke(&query, &folder, session_id, filter, show_score)?,
    }

    Ok(())
}
