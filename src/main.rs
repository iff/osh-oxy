use clap::{Parser, Subcommand};

pub(crate) mod commands;

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
        starttime: f64,
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
        } => commands::append_event::invoke(
            starttime, &command, &folder, endtime, exit_code, &machine, &session,
        )?,
        Command::Search {
            query,
            session_id,
            session_start,
        } => commands::search::invoke(&query, session_id, session_start).await?,
    }

    Ok(())
}
