use std::{
    process::{Command, Stdio},
    thread,
};

use anyhow::{Context, Result, bail};

use crate::{
    AddServerArgs, ListArgs,
    config::{add_server, config_path, init_config, load_or_create_config, remove_server},
    db::{
        find_workspace, load_workspaces, mark_server_missing, migrate, open_db, record_attach,
        set_alias_by_id, set_note_by_id, set_status_by_id, set_tags_by_id, upsert_workspace,
    },
    model::ServerConfig,
    remote::{
        attach_ssh_command, remote_doctor, remote_session_exists, scan_server, tmux_attach_command,
    },
    util::{edit_file, shell_quote, truncate},
};

pub fn scan() -> Result<()> {
    let summary = scan_index(true)?;
    println!("Indexed {} workspaces", summary.total);
    Ok(())
}

pub fn refresh_index_report() -> Result<ScanSummary> {
    scan_index(false)
}

#[derive(Debug, Clone)]
pub struct ScanSummary {
    pub total: usize,
    pub errors: Vec<String>,
}

fn scan_index(verbose: bool) -> Result<ScanSummary> {
    let config = load_or_create_config()?;
    let conn = open_db()?;
    migrate(&conn)?;

    let mut handles = Vec::new();
    for server in config.servers {
        if verbose {
            println!("Scanning {}...", server.name);
        }
        let server_name = server.name.clone();
        handles.push(thread::spawn(move || {
            let result = scan_server(&server);
            (server_name, result)
        }));
    }

    let mut total = 0;
    let mut errors = Vec::new();
    for handle in handles {
        let (server_name, result) = handle
            .join()
            .map_err(|_| anyhow::anyhow!("scan worker panicked"))?;
        match result {
            Ok(workspaces) => {
                mark_server_missing(&conn, &server_name)?;
                for workspace in &workspaces {
                    upsert_workspace(&conn, workspace)?;
                }
                total += workspaces.len();
            }
            Err(err) => {
                let message = format!("{server_name}: {err:#}");
                if verbose {
                    eprintln!("  failed: {message}");
                }
                errors.push(message);
            }
        }
    }

    Ok(ScanSummary { total, errors })
}

pub fn list_workspaces(args: &ListArgs) -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let workspaces = filtered_workspaces(load_workspaces(&conn)?, args);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&workspaces)?);
        return Ok(());
    }

    for ws in workspaces {
        let name = ws.alias.as_deref().unwrap_or(&ws.name);
        let tags = if ws.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", ws.tags.join(","))
        };
        println!(
            "{:<44} {:<22} {:<8} {:<8} {}{}{}{}",
            ws.id,
            truncate(name, 22),
            ws.agent,
            workspace_state(&ws),
            ws.root_path,
            git_summary(&ws),
            tags,
            agent_context_summary(&ws)
        );
    }
    Ok(())
}

fn workspace_state(ws: &crate::model::Workspace) -> String {
    if ws.presence == "seen" {
        ws.status.clone()
    } else {
        format!("{}/{}", ws.status, ws.presence)
    }
}

pub fn list_servers() -> Result<()> {
    let config = load_or_create_config()?;
    for server in config.servers {
        let kind = if server.local { "local" } else { "ssh" };
        let target = if server.local {
            String::from("-")
        } else {
            server.ssh
        };
        println!(
            "{:<24} {:<8} {:<18} {}",
            server.name,
            kind,
            server.term.as_deref().unwrap_or(""),
            target
        );
    }
    Ok(())
}

pub fn add_server_command(args: &AddServerArgs) -> Result<()> {
    let ssh = args.ssh.clone().unwrap_or_default();
    if !args.local && ssh.trim().is_empty() {
        bail!("remote server requires --ssh, for example: ws add-server prod --ssh 'ssh prod'");
    }
    if args.local && !ssh.trim().is_empty() {
        bail!("local server cannot also define --ssh");
    }

    add_server(ServerConfig {
        name: args.name.clone(),
        ssh,
        term: Some(args.term.clone()),
        local: args.local,
    })?;
    println!("Added server {}", args.name);
    Ok(())
}

pub fn remove_server_command(name: &str) -> Result<()> {
    remove_server(name)?;
    println!("Removed server {name}");
    Ok(())
}

fn git_summary(ws: &crate::model::Workspace) -> String {
    let Some(git) = &ws.git else {
        return String::new();
    };
    let mut parts = Vec::new();
    if let Some(branch) = &git.branch {
        if let Some(head) = &git.head {
            parts.push(format!("{branch}@{head}"));
        } else {
            parts.push(branch.clone());
        }
    } else if let Some(head) = &git.head {
        parts.push(format!("detached@{head}"));
    }
    if git.dirty {
        parts.push("dirty".to_string());
    } else {
        parts.push("clean".to_string());
    }
    if git.ahead > 0 {
        parts.push(format!("ahead {}", git.ahead));
    }
    if git.behind > 0 {
        parts.push(format!("behind {}", git.behind));
    }
    if let Some(remote) = &git.remote {
        parts.push(truncate(remote, 72));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {{{}}}", parts.join(", "))
    }
}

fn agent_context_summary(ws: &crate::model::Workspace) -> String {
    if ws.agent_context.is_empty() {
        String::new()
    } else {
        let files = ws
            .agent_context
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>()
            .join(",");
        format!(" <agent:{files}>")
    }
}

fn filtered_workspaces(
    workspaces: Vec<crate::model::Workspace>,
    args: &ListArgs,
) -> Vec<crate::model::Workspace> {
    workspaces
        .into_iter()
        .filter(|ws| args.all || ws.status != "archived")
        .filter(|ws| {
            args.server
                .as_ref()
                .is_none_or(|server| &ws.server == server)
        })
        .filter(|ws| {
            args.status
                .as_ref()
                .is_none_or(|status| &ws.status == status)
        })
        .collect()
}

pub fn open_config() -> Result<()> {
    let path = config_path()?;
    if !path.exists() {
        init_config()?;
    }
    edit_file(&path)
}

pub fn attach(name: &str) -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let ws =
        find_workspace(&conn, name)?.with_context(|| format!("workspace not found: {name}"))?;
    let config = load_or_create_config()?;
    let server = config
        .servers
        .iter()
        .find(|s| s.name == ws.server)
        .with_context(|| format!("server not found in config: {}", ws.server))?;

    if !remote_session_exists(server, &ws.session)? {
        bail!(
            "tmux session `{}` is missing on `{}`. Recreate it with: ws recreate {}",
            ws.session,
            ws.server,
            ws.id
        );
    }

    record_attach(&conn, &ws.id)?;
    let remote = tmux_attach_command(&ws.session, server.term.as_deref());
    let command = server_command_for_tty(server, &remote);
    Command::new("sh")
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to run attach command")?;
    Ok(())
}

pub fn list_agent_context(name: &str) -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let ws =
        find_workspace(&conn, name)?.with_context(|| format!("workspace not found: {name}"))?;

    if ws.agent_context.is_empty() {
        println!("No agent context files indexed for {}", ws.id);
        println!("Run `ws scan` after adding AGENTS.md, CLAUDE.md, or another supported file.");
        return Ok(());
    }

    println!("{} agent context", ws.id);
    for file in ws.agent_context {
        println!();
        println!("== {} ==", file.path);
        if !file.title.trim().is_empty() {
            println!("{}", file.title.trim());
            println!();
        }
        println!("{}", file.preview.trim());
    }
    Ok(())
}

pub fn recreate(name: &str) -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let ws =
        find_workspace(&conn, name)?.with_context(|| format!("workspace not found: {name}"))?;
    let config = load_or_create_config()?;
    let server = config
        .servers
        .iter()
        .find(|s| s.name == ws.server)
        .with_context(|| format!("server not found in config: {}", ws.server))?;

    let remote = format!(
        "cd {} && TERM={} tmux new-session -A -s {}",
        shell_quote(&ws.root_path),
        shell_quote(server.term.as_deref().unwrap_or("xterm-256color")),
        shell_quote(&ws.session)
    );
    let command = server_command_for_tty(server, &remote);
    Command::new("sh")
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to recreate workspace")?;
    Ok(())
}

fn server_command_for_tty(server: &crate::model::ServerConfig, command: &str) -> String {
    if server.local {
        command.to_string()
    } else {
        format!(
            "{} {}",
            attach_ssh_command(&server.ssh),
            shell_quote(command)
        )
    }
}

pub fn doctor() -> Result<()> {
    let config = load_or_create_config()?;
    let conn = open_db()?;
    migrate(&conn)?;
    let workspaces = load_workspaces(&conn)?;

    for server in &config.servers {
        println!("server: {}", server.name);
        match remote_doctor(server) {
            Ok(report) => {
                if server.local {
                    println!("  connection: local");
                } else {
                    println!("  ssh: ok");
                }
                println!("  host: {}", report.hostname);
                println!(
                    "  tmux: {}",
                    if report.tmux_available {
                        "ok"
                    } else {
                        "missing"
                    }
                );
                let mut server_workspaces: Vec<_> = workspaces
                    .iter()
                    .filter(|workspace| workspace.server == server.name)
                    .collect();
                server_workspaces.sort_by(|left, right| left.name.cmp(&right.name));
                for workspace in server_workspaces {
                    let status = if report.sessions.contains(&workspace.session) {
                        "ok"
                    } else {
                        "missing"
                    };
                    println!("  {:<40} {}", workspace.id, status);
                }
            }
            Err(err) => {
                println!("  ssh: failed");
                println!("  error: {err:#}");
            }
        }
    }
    Ok(())
}

pub fn set_note(name: &str, note: &str) -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let ws =
        find_workspace(&conn, name)?.with_context(|| format!("workspace not found: {name}"))?;
    let changed = set_note_by_id(&conn, &ws.id, note)?;
    if changed == 0 {
        bail!("workspace not found: {name}");
    }
    Ok(())
}

pub fn set_status(name: &str, status: &str) -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let ws =
        find_workspace(&conn, name)?.with_context(|| format!("workspace not found: {name}"))?;
    let changed = set_status_by_id(&conn, &ws.id, status)?;
    if changed == 0 {
        bail!("workspace not found: {name}");
    }
    Ok(())
}

pub fn set_alias(name: &str, alias: &str) -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let ws =
        find_workspace(&conn, name)?.with_context(|| format!("workspace not found: {name}"))?;
    let alias = alias.trim();
    let changed = if alias.is_empty() || alias == "-" {
        set_alias_by_id(&conn, &ws.id, None)?
    } else {
        set_alias_by_id(&conn, &ws.id, Some(alias))?
    };
    if changed == 0 {
        bail!("workspace not found: {name}");
    }
    Ok(())
}

pub fn set_tags(name: &str, tags: &[String]) -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let ws =
        find_workspace(&conn, name)?.with_context(|| format!("workspace not found: {name}"))?;
    let normalized: Vec<String> = tags
        .iter()
        .flat_map(|tag| tag.split(','))
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToString::to_string)
        .collect();
    let changed = set_tags_by_id(&conn, &ws.id, &normalized)?;
    if changed == 0 {
        bail!("workspace not found: {name}");
    }
    Ok(())
}
