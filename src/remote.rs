use std::{
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::{
    model::{DoctorReport, GitInfo, Pane, ServerConfig, Workspace},
    util::shell_quote,
};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(8);
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub fn scan_server(server: &ServerConfig) -> Result<Vec<Workspace>> {
    let format = "session=#{session_name}|window=#{window_index}:#{window_name}|pane=#{pane_index}|active=#{pane_active}|cmd=#{pane_current_command}|path=#{pane_current_path}|title=#{pane_title}";
    let command = format!("tmux list-panes -a -F {}", shell_quote(format));
    let output = run_server_command(server, &command).context("failed to run tmux scan")?;

    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }

    let panes: Vec<(String, Pane)> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_pane_line)
        .collect::<Result<_>>()?;
    let mut workspaces = group_panes(&server.name, panes);
    for workspace in &mut workspaces {
        workspace.git = scan_git(server, &workspace.root_path).ok().flatten();
    }
    Ok(workspaces)
}

pub fn remote_session_exists(server: &ServerConfig, session: &str) -> Result<bool> {
    let command = format!("tmux has-session -t {}", shell_quote(session));
    let output = run_server_command(server, &command)?;
    Ok(output.status.success())
}

pub fn remote_doctor(server: &ServerConfig) -> Result<DoctorReport> {
    let command = "printf 'hostname='; hostname; if command -v tmux >/dev/null 2>&1; then echo 'tmux=ok'; tmux list-sessions -F 'session=#{session_name}' 2>/dev/null || true; else echo 'tmux=missing'; fi";
    let output = run_server_command(server, command)?;
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

fn run_server_command(server: &ServerConfig, command: &str) -> Result<Output> {
    if server.local {
        return run_command_with_timeout(Command::new("sh").arg("-lc").arg(command), "local");
    }

    let command = format!("{} {}", server.ssh, shell_quote(command));
    run_command_with_timeout(
        Command::new("sh").arg("-lc").arg(command),
        &format!("remote {}", server.name),
    )
}

fn run_command_with_timeout(command: &mut Command, label: &str) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to run {label} command"))?;
    let started = Instant::now();

    loop {
        if child
            .try_wait()
            .with_context(|| format!("failed to wait for {label} command"))?
            .is_some()
        {
            return child
                .wait_with_output()
                .with_context(|| format!("failed to collect {label} command output"));
        }

        if started.elapsed() >= COMMAND_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait_with_output();
            bail!(
                "{label} command timed out after {}s",
                COMMAND_TIMEOUT.as_secs()
            );
        }

        thread::sleep(COMMAND_POLL_INTERVAL);
    }
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

fn group_panes(server: &str, rows: Vec<(String, Pane)>) -> Vec<Workspace> {
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
                alias: None,
                server: server.to_string(),
                session,
                root_path: agent_pane.path.clone(),
                agent,
                panes,
                note: String::new(),
                status: "active".to_string(),
                tags: Vec::new(),
                last_seen: Utc::now().to_rfc3339(),
                last_attached_at: None,
                attach_count: 0,
                git: None,
            })
        })
        .collect()
}

fn scan_git(server: &ServerConfig, path: &str) -> Result<Option<GitInfo>> {
    let command = format!(
        "cd {} 2>/dev/null && git rev-parse --is-inside-work-tree >/dev/null 2>&1 && branch=$(git branch --show-current 2>/dev/null || true) && head=$(git rev-parse --short HEAD 2>/dev/null || true) && remote=$(git remote get-url origin 2>/dev/null || true) && if [ -n \"$(git status --porcelain 2>/dev/null)\" ]; then dirty=1; else dirty=0; fi && counts=$(git rev-list --left-right --count '@{{upstream}}...HEAD' 2>/dev/null || printf '0\t0') && printf 'branch=%s\\nhead=%s\\nremote=%s\\ndirty=%s\\ncounts=%s\\n' \"$branch\" \"$head\" \"$remote\" \"$dirty\" \"$counts\"",
        shell_quote(path)
    );
    let output = run_server_command(server, &command)?;
    if !output.status.success() {
        return Ok(None);
    }

    let mut branch = None;
    let mut head = None;
    let mut remote = None;
    let mut dirty = false;
    let mut ahead = 0;
    let mut behind = 0;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(value) = line.strip_prefix("branch=") {
            branch = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        } else if let Some(value) = line.strip_prefix("head=") {
            head = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        } else if let Some(value) = line.strip_prefix("remote=") {
            remote = normalize_git_remote(value);
        } else if let Some(value) = line.strip_prefix("dirty=") {
            dirty = value == "1";
        } else if let Some(value) = line.strip_prefix("counts=") {
            let mut parts = value.split_whitespace();
            behind = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
            ahead = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
        }
    }

    Ok(Some(GitInfo {
        branch,
        head,
        remote,
        dirty,
        ahead,
        behind,
    }))
}

fn normalize_git_remote(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Some((host, path)) = value
        .strip_prefix("git@")
        .and_then(|rest| rest.split_once(':'))
    {
        return Some(format!(
            "https://{}/{}",
            host,
            path.trim_end_matches(".git")
        ));
    }

    if let Some(rest) = value.strip_prefix("ssh://git@") {
        if let Some((host, path)) = rest.split_once('/') {
            return Some(format!(
                "https://{}/{}",
                host,
                path.trim_end_matches(".git")
            ));
        }
    }

    Some(value.trim_end_matches(".git").to_string())
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

    use super::{attach_ssh_command, group_panes, normalize_git_remote, tmux_attach_command};

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

    #[test]
    fn normalizes_common_git_remote_urls() {
        assert_eq!(
            normalize_git_remote("git@github.com:user/repo.git").as_deref(),
            Some("https://github.com/user/repo")
        );
        assert_eq!(
            normalize_git_remote("ssh://git@github.com/user/repo.git").as_deref(),
            Some("https://github.com/user/repo")
        );
        assert_eq!(
            normalize_git_remote("https://github.com/user/repo.git").as_deref(),
            Some("https://github.com/user/repo")
        );
    }
}
