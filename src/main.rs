use clap::{Parser, Subcommand};

pub(crate) mod async_binary_writer;
pub(crate) mod commands;
pub(crate) mod event;
pub(crate) mod json_lines;

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
    Convert {
        #[arg(long)]
        path: String,
    },
    Sk {
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        session_id: Option<String>,
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
        } => {
            commands::append_event::invoke(
                starttime, &command, &folder, endtime, exit_code, &machine, &session,
            )
            .await?
        }
        Command::Convert { path } => commands::convert::invoke(&path).await?,
        Command::Sk { query, session_id } => commands::sk::invoke(&query, session_id).await?,
    }

    Ok(())
}
