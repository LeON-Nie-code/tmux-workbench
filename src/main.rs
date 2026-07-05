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
    add_server_command, attach, doctor, list_agent_context, list_servers, list_workspaces,
    open_config, recreate, remove_server_command, scan, set_alias, set_note, set_status, set_tags,
};
use config::init_config;
use tui::run_tui;

#[derive(Parser)]
#[command(name = "ws")]
#[command(about = "Tmux Workbench: workspace memory for local and remote tmux sessions")]
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
    Agent {
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
    Servers,
    AddServer(AddServerArgs),
    RemoveServer {
        name: String,
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

#[derive(Debug, Clone, Args)]
pub struct AddServerArgs {
    pub name: String,
    #[arg(long)]
    pub ssh: Option<String>,
    #[arg(long)]
    pub local: bool,
    #[arg(long, default_value = "xterm-256color")]
    pub term: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Init) => init_config(),
        Some(Commands::Scan) => scan(),
        Some(Commands::List(args)) => list_workspaces(&args),
        Some(Commands::OpenConfig) => open_config(),
        Some(Commands::Attach { workspace }) => attach(&workspace),
        Some(Commands::Agent { workspace }) => list_agent_context(&workspace),
        Some(Commands::Recreate { workspace }) => recreate(&workspace),
        Some(Commands::Note { workspace, note }) => set_note(&workspace, &note),
        Some(Commands::Status { workspace, status }) => set_status(&workspace, &status),
        Some(Commands::Alias { workspace, alias }) => set_alias(&workspace, &alias),
        Some(Commands::Tags { workspace, tags }) => set_tags(&workspace, &tags),
        Some(Commands::Servers) => list_servers(),
        Some(Commands::AddServer(args)) => add_server_command(&args),
        Some(Commands::RemoveServer { name }) => remove_server_command(&name),
        Some(Commands::Doctor) => doctor(),
        None => run_tui(),
    }
}
