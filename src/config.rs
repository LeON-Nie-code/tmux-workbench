use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail};

use crate::model::{Config, ServerConfig};

pub fn init_config() -> Result<()> {
    let path = config_path()?;
    if path.exists() {
        println!("Config already exists: {}", path.display());
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let config = Config {
        servers: vec![ServerConfig {
            name: "local".to_string(),
            ssh: String::new(),
            term: Some("xterm-256color".to_string()),
            local: true,
        }],
    };

    fs::write(&path, serde_yaml::to_string(&config)?)?;
    println!("Created {}", path.display());
    println!(
        "Next: run `ws scan`, or add a remote server with `ws add-server prod --ssh \"ssh prod\"`."
    );
    Ok(())
}

pub fn load_or_create_config() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        init_config()?;
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut config: Config =
        serde_yaml::from_str(&raw).with_context(|| format!("invalid config {}", path.display()))?;
    if !config.servers.iter().any(|server| server.local) {
        config.servers.insert(
            0,
            ServerConfig {
                name: "local".to_string(),
                ssh: String::new(),
                term: Some("xterm-256color".to_string()),
                local: true,
            },
        );
    }
    Ok(config)
}

pub fn load_config_file() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        init_config()?;
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&raw).with_context(|| format!("invalid config {}", path.display()))
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_yaml::to_string(config)?)?;
    Ok(())
}

pub fn add_server(server: ServerConfig) -> Result<()> {
    let mut config = load_config_file()?;
    if config.servers.iter().any(|item| item.name == server.name) {
        bail!("server already exists: {}", server.name);
    }
    config.servers.push(server);
    save_config(&config)
}

pub fn remove_server(name: &str) -> Result<()> {
    let mut config = load_config_file()?;
    let before = config.servers.len();
    config.servers.retain(|server| server.name != name);
    if config.servers.len() == before {
        bail!("server not found: {name}");
    }
    save_config(&config)
}

pub fn config_path() -> Result<PathBuf> {
    Ok(home_dir()?.join(".config").join("ws").join("config.yaml"))
}

pub fn data_path() -> Result<PathBuf> {
    Ok(home_dir()?.join(".local").join("share").join("ws"))
}

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("no home directory found")
}
