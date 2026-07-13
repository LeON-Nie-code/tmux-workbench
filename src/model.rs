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
    #[serde(default)]
    pub local: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub alias: Option<String>,
    pub server: String,
    pub session: String,
    pub root_path: String,
    pub agent: String,
    pub panes: Vec<Pane>,
    pub note: String,
    pub status: String,
    pub presence: String,
    pub tags: Vec<String>,
    pub last_seen: String,
    pub last_attached_at: Option<String>,
    pub attach_count: i64,
    pub git: Option<GitInfo>,
    pub agent_context: Vec<AgentContextFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContextFile {
    pub path: String,
    pub title: String,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitInfo {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub remote: Option<String>,
    pub dirty: bool,
    pub ahead: i64,
    pub behind: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pane {
    pub window: String,
    pub layout: String,
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
