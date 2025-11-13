use clap::{Parser, Subcommand};
use osh_oxy::commands;

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
    Cat {
        #[arg(long)]
        unique: bool,
    },
    Convert {},
    Search {
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        folder: String,
        #[arg(long)]
        session_id: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Cat { unique } => commands::cat::invoke(unique).await?,
        Command::AppendEvent {
            starttime,
            command,
            folder,
            endtime,
            exit_code,
            machine,
            session,
        } => {
            commands::append_event::invoke(
                starttime, &command, &folder, endtime, exit_code, &machine, &session,
            )
            .await?
        }
        Command::Convert {} => commands::convert::invoke().await?,
        Command::Search {
            query,
            folder,
            session_id,
        } => commands::search::invoke(&query, &folder, session_id).await?,
    }

    Ok(())
}
