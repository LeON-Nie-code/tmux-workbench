use std::{fs, path::PathBuf};

use anyhow::{Context, Result};

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
        servers: vec![
            ServerConfig {
                name: "cavelight-local-frp".to_string(),
                ssh: "ssh cavelight-local-frp".to_string(),
                term: Some("xterm-256color".to_string()),
            },
            ServerConfig {
                name: "AI-Teacher-Baidu".to_string(),
                ssh: "ssh AI-Teacher-Baidu".to_string(),
                term: Some("xterm-256color".to_string()),
            },
            ServerConfig {
                name: "gcloud-emflux".to_string(),
                ssh: "ssh instance-20260624-045641.asia-southeast1-b.emflux".to_string(),
                term: Some("xterm-256color".to_string()),
            },
        ],
    };

    fs::write(&path, serde_yaml::to_string(&config)?)?;
    println!("Created {}", path.display());
    Ok(())
}

pub fn load_or_create_config() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        init_config()?;
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&raw).with_context(|| format!("invalid config {}", path.display()))
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
