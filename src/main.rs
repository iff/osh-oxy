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
        session_id: Option<String>,
        #[arg(long)]
        unique: bool,
    },
    Convert {},
    Ui {
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        unique: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Cat { session_id, unique } => commands::cat::invoke(session_id, unique).await?,
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
        Command::Ui {
            query,
            session_id,
            unique,
        } => commands::tui::invoke(&query, session_id, unique).await?,
    }

    Ok(())
}
