use std::process::{Command, Output};

use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::{
    model::{DoctorReport, Pane, ServerConfig, Workspace},
    util::shell_quote,
};

pub fn scan_server(server: &ServerConfig) -> Result<Vec<(String, Pane)>> {
    let format = "session=#{session_name}|window=#{window_index}:#{window_name}|pane=#{pane_index}|active=#{pane_active}|cmd=#{pane_current_command}|path=#{pane_current_path}|title=#{pane_title}";
    let remote = format!("tmux list-panes -a -F {}", shell_quote(format));
    let command = format!("{} {}", server.ssh, shell_quote(&remote));
    let output = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .output()
        .context("failed to run ssh scan")?;

    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_pane_line)
        .collect()
}

pub fn remote_session_exists(server: &ServerConfig, session: &str) -> Result<bool> {
    let remote = format!("tmux has-session -t {}", shell_quote(session));
    let output = run_remote(server, &remote)?;
    Ok(output.status.success())
}

pub fn remote_doctor(server: &ServerConfig) -> Result<DoctorReport> {
    let remote = "printf 'hostname='; hostname; if command -v tmux >/dev/null 2>&1; then echo 'tmux=ok'; tmux list-sessions -F 'session=#{session_name}' 2>/dev/null || true; else echo 'tmux=missing'; fi";
    let output = run_remote(server, remote)?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }

    let mut hostname = String::from("unknown");
    let mut tmux_available = false;
    let mut sessions = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(value) = line.strip_prefix("hostname=") {
            hostname = value.to_string();
        } else if let Some(value) = line.strip_prefix("tmux=") {
            tmux_available = value == "ok";
        } else if let Some(value) = line.strip_prefix("session=") {
            sessions.push(value.to_string());
        }
    }

    Ok(DoctorReport {
        hostname,
        tmux_available,
        sessions,
    })
}

fn run_remote(server: &ServerConfig, remote: &str) -> Result<Output> {
    let command = format!("{} {}", server.ssh, shell_quote(remote));
    Command::new("sh")
        .arg("-lc")
        .arg(command)
        .output()
        .context("failed to run remote command")
}

fn parse_pane_line(line: &str) -> Result<(String, Pane)> {
    let mut session = String::new();
    let mut window = String::new();
    let mut pane = 0;
    let mut active = false;
    let mut command = String::new();
    let mut path = String::new();
    let mut title = String::new();

    for part in line.split('|') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "session" => session = value.to_string(),
            "window" => window = value.to_string(),
            "pane" => pane = value.parse().unwrap_or(0),
            "active" => active = value == "1",
            "cmd" => command = value.to_string(),
            "path" => path = value.to_string(),
            "title" => title = value.to_string(),
            _ => {}
        }
    }

    if session.is_empty() {
        bail!("invalid tmux pane line: {line}");
    }

    Ok((
        session,
        Pane {
            window,
            pane,
            active,
            command,
            path,
            title,
        },
    ))
}

pub fn group_panes(server: &str, rows: Vec<(String, Pane)>) -> Vec<Workspace> {
    let mut names = Vec::<String>::new();
    let mut grouped = std::collections::BTreeMap::<String, Vec<Pane>>::new();
    for (session, pane) in rows {
        if !grouped.contains_key(&session) {
            names.push(session.clone());
        }
        grouped.entry(session).or_default().push(pane);
    }

    names
        .into_iter()
        .filter_map(|session| {
            let panes = grouped.remove(&session)?;
            let active = panes.iter().find(|pane| pane.active).unwrap_or(&panes[0]);
            let agent_pane = panes
                .iter()
                .find(|pane| pane.command == "codex" || pane.command == "claude")
                .unwrap_or(active);
            let agent = agent_pane.command.clone();
            Some(Workspace {
                id: format!("{server}/{}", session),
                name: session.clone(),
                server: server.to_string(),
                session,
                root_path: agent_pane.path.clone(),
                agent,
                panes,
                note: String::new(),
                status: "active".to_string(),
                last_seen: Utc::now().to_rfc3339(),
                last_attached_at: None,
                attach_count: 0,
            })
        })
        .collect()
}

pub fn tmux_attach_command(session: &str, term: Option<&str>) -> String {
    let term = term.unwrap_or("xterm-256color");
    format!(
        "TERM={} tmux attach -t {}",
        shell_quote(term),
        shell_quote(session)
    )
}

pub fn attach_ssh_command(ssh: &str) -> String {
    let trimmed = ssh.trim();
    if trimmed == "ssh" {
        return "ssh -t".to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("ssh ") {
        if rest
            .split_whitespace()
            .any(|part| part == "-t" || part == "-tt")
        {
            trimmed.to_string()
        } else {
            format!("ssh -t {rest}")
        }
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Pane;

    use super::{attach_ssh_command, group_panes, tmux_attach_command};

    #[test]
    fn adds_tty_to_plain_ssh_attach_command() {
        assert_eq!(
            attach_ssh_command("ssh AI-Teacher-Baidu"),
            "ssh -t AI-Teacher-Baidu"
        );
        assert_eq!(
            attach_ssh_command("ssh -t cavelight-local-frp"),
            "ssh -t cavelight-local-frp"
        );
    }

    #[test]
    fn attach_command_sets_terminal_fallback() {
        assert_eq!(
            tmux_attach_command("NeuroPlay", None),
            "TERM='xterm-256color' tmux attach -t 'NeuroPlay'"
        );
    }

    #[test]
    fn workspace_root_prefers_agent_pane_path() {
        let workspaces = group_panes(
            "server",
            vec![
                (
                    "demo".to_string(),
                    Pane {
                        window: "0:main".to_string(),
                        pane: 0,
                        active: false,
                        command: "claude".to_string(),
                        path: "/repo".to_string(),
                        title: String::new(),
                    },
                ),
                (
                    "demo".to_string(),
                    Pane {
                        window: "0:main".to_string(),
                        pane: 1,
                        active: true,
                        command: "bash".to_string(),
                        path: "/repo/frontend".to_string(),
                        title: String::new(),
                    },
                ),
            ],
        );

        assert_eq!(workspaces[0].root_path, "/repo");
        assert_eq!(workspaces[0].agent, "claude");
    }
}
