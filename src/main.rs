use anyhow::Result;
use clap::{Args, Parser, Subcommand};

mod commands;
mod config;
mod db;
mod model;
mod remote;
mod tui;
mod util;

use commands::{
    attach, doctor, list_workspaces, open_config, recreate, scan, set_alias, set_note, set_status,
    set_tags,
};
use config::init_config;
use tui::run_tui;

#[derive(Parser)]
#[command(name = "ws")]
#[command(about = "Remote workspace memory manager")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Scan,
    List(ListArgs),
    OpenConfig,
    Attach {
        workspace: String,
    },
    Recreate {
        workspace: String,
    },
    Note {
        workspace: String,
        note: String,
    },
    Status {
        workspace: String,
        status: String,
    },
    Alias {
        workspace: String,
        alias: String,
    },
    Tags {
        workspace: String,
        tags: Vec<String>,
    },
    Doctor,
}

#[derive(Debug, Clone, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub server: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub json: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Init) => init_config(),
        Some(Commands::Scan) => scan(),
        Some(Commands::List(args)) => list_workspaces(&args),
        Some(Commands::OpenConfig) => open_config(),
        Some(Commands::Attach { workspace }) => attach(&workspace),
        Some(Commands::Recreate { workspace }) => recreate(&workspace),
        Some(Commands::Note { workspace, note }) => set_note(&workspace, &note),
        Some(Commands::Status { workspace, status }) => set_status(&workspace, &status),
        Some(Commands::Alias { workspace, alias }) => set_alias(&workspace, &alias),
        Some(Commands::Tags { workspace, tags }) => set_tags(&workspace, &tags),
        Some(Commands::Doctor) => doctor(),
        None => run_tui(),
    }
}
