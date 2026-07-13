use std::{
    io::{self, Write},
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
    model::{Pane, ServerConfig, Workspace},
    remote::{
        attach_ssh_command, remote_doctor, remote_session_exists, scan_server, tmux_attach_command,
    },
    util::{edit_file, shell_quote, truncate},
};

pub fn scan() -> Result<()> {
    let summary = scan_index(true)?;
    println!("Indexed {} workspaces", summary.total);
    if !summary.errors.is_empty() {
        println!(
            "{} server(s) failed. Run `ws doctor` for details.",
            summary.errors.len()
        );
    }
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
    let command = attach_command_for_workspace(name)?;
    run_attach_command(name, &command)
}

pub fn attach_command_for_workspace(name: &str) -> Result<String> {
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
    Ok(server_command_for_tty(server, &remote))
}

pub fn run_attach_command(workspace: &str, command: &str) -> Result<()> {
    print!("\x1b[0m\x1b[?25h");
    println!("Opening {workspace}...");
    println!("Connection prepared. Handing off to SSH/tmux now.");
    println!("If the terminal stays here, check the server with `ws doctor`.");
    io::stdout().flush().ok();
    let status = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to run attach command")?;
    if !status.success() {
        bail!("attach command exited with {status}");
    }
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

    if remote_session_exists(server, &ws.session)? {
        println!(
            "Workspace {} already exists; attaching without replacing it.",
            ws.id
        );
    } else {
        let restore = build_recreate_command(&ws, server.term.as_deref());
        let command = server_command_for_tty(server, &restore);
        let status = Command::new("sh")
            .arg("-lc")
            .arg(command)
            .status()
            .context("failed to restore workspace snapshot")?;
        if !status.success() {
            bail!("failed to restore workspace snapshot");
        }
        println!(
            "Restored {} window(s) and {} pane(s) from the last scan.",
            unique_windows(&ws.panes).len(),
            ws.panes.len()
        );
        if ws
            .panes
            .iter()
            .any(|pane| restorable_foreground_command(pane).is_some())
        {
            println!(
                "Foreground commands were restored without running them; press Enter in each pane when ready."
            );
        }
    }

    let remote = tmux_attach_command(&ws.session, server.term.as_deref());
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

fn build_recreate_command(ws: &Workspace, term: Option<&str>) -> String {
    let windows = unique_windows(&ws.panes);
    if windows.is_empty() {
        return format!(
            "TERM={} tmux new-session -d -s {} -c {}",
            shell_quote(term.unwrap_or("xterm-256color")),
            shell_quote(&ws.session),
            shell_quote(&ws.root_path),
        );
    }

    let session = shell_quote(&ws.session);
    let mut commands = vec![format!(
        "export TERM={}",
        shell_quote(term.unwrap_or("xterm-256color"))
    )];
    for (window_position, window) in windows.iter().enumerate() {
        let panes: Vec<&Pane> = ws
            .panes
            .iter()
            .filter(|pane| &pane.window == window)
            .collect();
        let (_, window_name) = window.split_once(':').unwrap_or((window, window));
        let first = panes[0];
        if window_position == 0 {
            commands.push(format!(
                "tmux new-session -d -s {session} -n {} -c {}",
                shell_quote(window_name),
                shell_quote(&first.path),
            ));
        } else {
            commands.push(format!(
                "tmux new-window -d -t {session} -n {} -c {}",
                shell_quote(window_name),
                shell_quote(&first.path),
            ));
        }
        for pane in panes.iter().skip(1) {
            commands.push(format!(
                "tmux split-window -d -t {} -c {}",
                shell_quote(&format!("{}:{}", ws.session, window_name)),
                shell_quote(&pane.path),
            ));
        }
        if !first.layout.is_empty() {
            commands.push(format!(
                "tmux select-layout -t {} {} >/dev/null",
                shell_quote(&format!("{}:{}", ws.session, window_name)),
                shell_quote(&first.layout),
            ));
        }
        for pane in &panes {
            if let Some(command) = restorable_foreground_command(pane) {
                commands.push(format!(
                    "tmux send-keys -l -t {} {}",
                    shell_quote(&format!("{}:{}.{}", ws.session, window_name, pane.pane)),
                    shell_quote(command),
                ));
            }
        }
        if let Some(active) = panes.iter().find(|pane| pane.active) {
            commands.push(format!(
                "tmux select-pane -t {}",
                shell_quote(&format!("{}:{}.{}", ws.session, window_name, active.pane)),
            ));
        }
    }
    commands.join(" && ")
}

fn unique_windows(panes: &[Pane]) -> Vec<String> {
    let mut windows = Vec::new();
    for pane in panes {
        if !windows.contains(&pane.window) {
            windows.push(pane.window.clone());
        }
    }
    windows
}

fn restorable_foreground_command(pane: &Pane) -> Option<&str> {
    if is_shell_process(&pane.command) {
        None
    } else {
        Some(restorable_command_name(&pane.command))
    }
}

fn restorable_command_name(command: &str) -> &str {
    if command.starts_with("codex") {
        "codex"
    } else if command.starts_with("claude") {
        "claude"
    } else if command.starts_with("gemini") {
        "gemini"
    } else {
        command
    }
}

fn is_shell_process(command: &str) -> bool {
    matches!(command, "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh")
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

    println!("Tmux Workbench doctor");
    println!();
    println!("Local environment");
    println!("  config: {}", config_path()?.display());
    println!(
        "  database: {}",
        crate::config::data_path()?.join("workspaces.db").display()
    );
    println!(
        "  tmux: {}",
        if command_available("tmux") {
            "ok"
        } else {
            "missing"
        }
    );
    println!(
        "  ssh: {}",
        if command_available("ssh") {
            "ok"
        } else {
            "missing"
        }
    );
    println!(
        "  git: {}",
        if command_available("git") {
            "ok"
        } else {
            "missing"
        }
    );
    println!("  indexed workspaces: {}", workspaces.len());
    println!();

    if config.servers.is_empty() {
        println!("No servers configured.");
        println!("Add local tmux indexing with `ws add-server local --local`.");
        println!("Add SSH indexing with `ws add-server prod --ssh \"ssh prod\"`.");
        return Ok(());
    }

    println!("Servers");
    for server in &config.servers {
        println!("  {}", server.name);
        match remote_doctor(server) {
            Ok(report) => {
                if server.local {
                    println!("    connection: local");
                } else {
                    println!("    ssh: ok");
                }
                println!("    host: {}", report.hostname);
                println!(
                    "    tmux: {}",
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
                println!("    indexed workspaces: {}", server_workspaces.len());
                for workspace in server_workspaces {
                    let status = if report.sessions.contains(&workspace.session) {
                        "ok"
                    } else {
                        "missing"
                    };
                    println!("    {:<40} {}", workspace.id, status);
                }
            }
            Err(err) => {
                if server.local {
                    println!("    local check: failed");
                } else {
                    println!("    ssh: failed");
                }
                println!("    error: {err:#}");
                println!(
                    "    hint: verify the command shown by `ws servers`, then run it directly."
                );
            }
        }
        println!();
    }
    Ok(())
}

fn command_available(name: &str) -> bool {
    Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {}", shell_quote(name)))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn print_stats() -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let mut workspaces = load_workspaces(&conn)?;
    workspaces.sort_by(|left, right| {
        right
            .attach_count
            .cmp(&left.attach_count)
            .then_with(|| right.last_attached_at.cmp(&left.last_attached_at))
            .then_with(|| left.id.cmp(&right.id))
    });

    let total = workspaces.len();
    let active = workspaces
        .iter()
        .filter(|workspace| workspace.status != "archived")
        .count();
    let archived = workspaces
        .iter()
        .filter(|workspace| workspace.status == "archived")
        .count();
    let missing = workspaces
        .iter()
        .filter(|workspace| workspace.presence != "seen")
        .count();
    let attaches: i64 = workspaces
        .iter()
        .map(|workspace| workspace.attach_count)
        .sum();

    println!("Workspace stats");
    println!("  total:     {total}");
    println!("  active:    {active}");
    println!("  archived:  {archived}");
    println!("  missing:   {missing}");
    println!("  attaches:  {attaches}");
    println!();

    println!("Top workspaces");
    for workspace in workspaces.iter().take(10) {
        println!(
            "  {:<42} {:>4} attach(es)  last: {}",
            truncate(&workspace.id, 42),
            workspace.attach_count,
            workspace.last_attached_at.as_deref().unwrap_or("never")
        );
    }

    let mut by_server = std::collections::BTreeMap::<&str, (usize, i64)>::new();
    for workspace in &workspaces {
        let entry = by_server.entry(&workspace.server).or_default();
        entry.0 += 1;
        entry.1 += workspace.attach_count;
    }

    println!();
    println!("By server");
    for (server, (count, attach_count)) in by_server {
        println!(
            "  {:<24} {:>4} workspace(s)  {:>4} attach(es)",
            server, count, attach_count
        );
    }

    let stale_count = workspaces
        .iter()
        .filter(|workspace| workspace.presence != "seen" && workspace.status != "archived")
        .count();
    if stale_count > 0 {
        println!();
        println!(
            "Tip: {stale_count} active workspace(s) are missing on their tmux server. Use `ws` to archive them or `ws recreate <workspace>` to restore one."
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recreate_command_restores_windows_panes_layouts_and_processes() {
        let workspace = Workspace {
            id: "local/demo".to_string(),
            name: "demo".to_string(),
            alias: None,
            server: "local".to_string(),
            session: "demo".to_string(),
            root_path: "/repo".to_string(),
            agent: "codex".to_string(),
            panes: vec![
                pane("0:code", 0, true, "zsh", "/repo", "layout-a"),
                pane("0:code", 1, false, "codex", "/repo", "layout-a"),
                pane("1:monitor", 0, true, "btop", "/tmp", "layout-b"),
            ],
            note: String::new(),
            status: "active".to_string(),
            presence: "missing".to_string(),
            tags: Vec::new(),
            last_seen: "now".to_string(),
            last_attached_at: None,
            attach_count: 0,
            git: None,
            agent_context: Vec::new(),
        };

        let command = build_recreate_command(&workspace, None);
        assert!(command.contains("tmux new-session -d"));
        assert!(command.contains("tmux new-window -d"));
        assert!(command.contains("tmux split-window -d"));
        assert!(command.contains("tmux select-layout"));
        assert!(command.contains("tmux send-keys -l"));
        assert!(command.contains("'codex'"));
        assert!(command.contains("'btop'"));
        assert!(!command.contains(" send-keys -l -t 'demo:code.0' 'zsh'"));
        assert!(!command.contains(" send-keys -l -t 'demo:code.1' 'codex' Enter"));
    }

    #[test]
    fn recreate_uses_portable_agent_command_names() {
        let pane = pane("0:code", 0, true, "codex-aarch64-a", "/repo", "");
        let command = restorable_foreground_command(&pane);
        assert_eq!(command, Some("codex"));
    }

    fn pane(
        window: &str,
        index: i64,
        active: bool,
        command: &str,
        path: &str,
        layout: &str,
    ) -> Pane {
        Pane {
            window: window.to_string(),
            layout: layout.to_string(),
            pane: index,
            active,
            command: command.to_string(),
            path: path.to_string(),
            title: String::new(),
        }
    }
}
