use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub servers: Vec<ServerConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub ssh: String,
    pub term: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub server: String,
    pub session: String,
    pub root_path: String,
    pub agent: String,
    pub panes: Vec<Pane>,
    pub note: String,
    pub status: String,
    pub last_seen: String,
    pub last_attached_at: Option<String>,
    pub attach_count: i64,
}

#[derive(Debug, Clone)]
pub struct Pane {
    pub window: String,
    pub pane: i64,
    pub active: bool,
    pub command: String,
    pub path: String,
    pub title: String,
}

#[derive(Debug)]
pub struct DoctorReport {
    pub hostname: String,
    pub tmux_available: bool,
    pub sessions: Vec<String>,
}
