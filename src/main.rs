use std::{
    fs,
    io::{self, Stdout, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

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
    List,
    OpenConfig,
    Attach { workspace: String },
    Recreate { workspace: String },
    Note { workspace: String, note: String },
    Status { workspace: String, status: String },
    Doctor,
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    servers: Vec<ServerConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ServerConfig {
    name: String,
    ssh: String,
    term: Option<String>,
}

#[derive(Debug, Clone)]
struct Workspace {
    id: String,
    name: String,
    server: String,
    session: String,
    root_path: String,
    agent: String,
    panes: Vec<Pane>,
    note: String,
    status: String,
    last_seen: String,
    last_attached_at: Option<String>,
    attach_count: i64,
}

#[derive(Debug, Clone)]
struct Pane {
    window: String,
    pane: i64,
    active: bool,
    command: String,
    path: String,
    title: String,
}

#[derive(Debug)]
struct DoctorReport {
    hostname: String,
    tmux_available: bool,
    sessions: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Init) => init_config(),
        Some(Commands::Scan) => scan(),
        Some(Commands::List) => list_workspaces(),
        Some(Commands::OpenConfig) => open_config(),
        Some(Commands::Attach { workspace }) => attach(&workspace),
        Some(Commands::Recreate { workspace }) => recreate(&workspace),
        Some(Commands::Note { workspace, note }) => set_note(&workspace, &note),
        Some(Commands::Status { workspace, status }) => set_status(&workspace, &status),
        Some(Commands::Doctor) => doctor(),
        None => run_tui(),
    }
}

fn init_config() -> Result<()> {
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

fn scan() -> Result<()> {
    let config = load_or_create_config()?;
    let conn = open_db()?;
    migrate(&conn)?;

    let mut total = 0;
    for server in config.servers {
        println!("Scanning {}...", server.name);
        match scan_server(&server) {
            Ok(panes) => {
                let workspaces = group_panes(&server.name, panes);
                for workspace in &workspaces {
                    upsert_workspace(&conn, workspace)?;
                }
                total += workspaces.len();
            }
            Err(err) => {
                eprintln!("  failed: {err:#}");
            }
        }
    }

    println!("Indexed {total} workspaces");
    Ok(())
}

fn list_workspaces() -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    for ws in load_workspaces(&conn)? {
        println!("{:<44} {:<8} {}", ws.id, ws.agent, ws.root_path);
    }
    Ok(())
}

fn open_config() -> Result<()> {
    let path = config_path()?;
    if !path.exists() {
        init_config()?;
    }
    edit_file(&path)
}

fn attach(name: &str) -> Result<()> {
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
    let command = format!(
        "{} {}",
        attach_ssh_command(&server.ssh),
        shell_quote(&remote)
    );
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

fn recreate(name: &str) -> Result<()> {
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
    let command = format!(
        "{} {}",
        attach_ssh_command(&server.ssh),
        shell_quote(&remote)
    );
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

fn doctor() -> Result<()> {
    let config = load_or_create_config()?;
    let conn = open_db()?;
    migrate(&conn)?;
    let workspaces = load_workspaces(&conn)?;

    for server in &config.servers {
        println!("server: {}", server.name);
        match remote_doctor(server) {
            Ok(report) => {
                println!("  ssh: ok");
                println!("  host: {}", report.hostname);
                println!(
                    "  tmux: {}",
                    if report.tmux_available {
                        "ok"
                    } else {
                        "missing"
                    }
                );
                let mut server_workspaces: Vec<&Workspace> = workspaces
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

fn set_note(name: &str, note: &str) -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let ws =
        find_workspace(&conn, name)?.with_context(|| format!("workspace not found: {name}"))?;
    let changed = conn.execute(
        "update workspaces set note = ?1 where id = ?2",
        params![note, ws.id],
    )?;
    if changed == 0 {
        bail!("workspace not found: {name}");
    }
    Ok(())
}

fn set_status(name: &str, status: &str) -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let ws =
        find_workspace(&conn, name)?.with_context(|| format!("workspace not found: {name}"))?;
    let changed = conn.execute(
        "update workspaces set status = ?1 where id = ?2",
        params![status, ws.id],
    )?;
    if changed == 0 {
        bail!("workspace not found: {name}");
    }
    Ok(())
}

fn record_attach(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "update workspaces
         set last_attached_at = ?1,
             attach_count = attach_count + 1
         where id = ?2",
        params![Utc::now().to_rfc3339(), id],
    )?;
    Ok(())
}

fn run_tui() -> Result<()> {
    let conn = open_db()?;
    migrate(&conn)?;
    let workspaces = load_workspaces(&conn)?;
    if workspaces.is_empty() {
        println!("No workspaces indexed yet. Run `ws init` and `ws scan` first.");
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = draw_tui(&mut terminal, workspaces);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}

fn draw_tui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut workspaces: Vec<Workspace>,
) -> Result<()> {
    let mut state = ListState::default();
    state.select(Some(0));
    let mut search = String::new();
    let mut mode = InputMode::Normal;
    let mut show_archived = false;

    loop {
        let filtered = filtered_indices(&workspaces, &search, show_archived);
        if filtered.is_empty() {
            state.select(None);
        } else {
            let selected = state.selected().unwrap_or(0).min(filtered.len() - 1);
            state.select(Some(selected));
        }

        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
                .split(frame.area());

            let title = match mode {
                InputMode::Normal if search.is_empty() => workspace_list_title(show_archived),
                InputMode::Normal => format!("{}  /{search}", workspace_list_title(show_archived)),
                InputMode::Search => format!("Search  /{search}"),
            };
            let items: Vec<ListItem> = filtered
                .iter()
                .map(|index| &workspaces[*index])
                .map(|ws| {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{:<22}", truncate(&ws.name, 22)),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!("{:<20}", truncate(&ws.server, 20))),
                        Span::raw(format!("{:<8}", truncate(&ws.agent, 8))),
                        Span::raw(format!("{:<8}", truncate(&ws.status, 8))),
                    ]))
                })
                .collect();
            let list = List::new(items)
                .block(Block::default().title(title).borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
            frame.render_stateful_widget(list, chunks[0], &mut state);

            let mut lines = if let Some(selected) = state.selected() {
                let ws = &workspaces[filtered[selected]];
                workspace_detail_lines(ws)
            } else {
                vec![Line::from("No matching workspaces")]
            };
            lines.push(Line::from(""));
            lines.push(Line::from(match mode {
                InputMode::Normal => {
                    "Enter attach  / search  n note  a archive  z show archived  r rescan  q quit"
                }
                InputMode::Search => "Type to search  Enter accept  Esc clear",
            }));

            let detail =
                Paragraph::new(lines).block(Block::default().title("Detail").borders(Borders::ALL));
            frame.render_widget(detail, chunks[1]);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('/') => {
                            mode = InputMode::Search;
                        }
                        KeyCode::Char('n') => {
                            if let Some(selected) = state.selected() {
                                let index = filtered[selected];
                                edit_note_from_tui(terminal, &mut workspaces[index])?;
                            }
                        }
                        KeyCode::Char('a') => {
                            if let Some(selected) = state.selected() {
                                let index = filtered[selected];
                                toggle_archive(&mut workspaces[index])?;
                                state.select(Some(0));
                            }
                        }
                        KeyCode::Char('z') => {
                            show_archived = !show_archived;
                            state.select(Some(0));
                        }
                        KeyCode::Char('r') => {
                            workspaces = rescan_from_tui(terminal)?;
                            state.select(Some(0));
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            if !filtered.is_empty() {
                                let next = state
                                    .selected()
                                    .unwrap_or(0)
                                    .saturating_add(1)
                                    .min(filtered.len() - 1);
                                state.select(Some(next));
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            let prev = state.selected().unwrap_or(0).saturating_sub(1);
                            state.select(Some(prev));
                        }
                        KeyCode::Enter => {
                            if let Some(selected) = state.selected() {
                                let ws = &workspaces[filtered[selected]];
                                disable_raw_mode()?;
                                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                return attach(&ws.id);
                            }
                        }
                        _ => {}
                    },
                    InputMode::Search => match key.code {
                        KeyCode::Esc => {
                            search.clear();
                            state.select(Some(0));
                            mode = InputMode::Normal;
                        }
                        KeyCode::Enter => {
                            mode = InputMode::Normal;
                        }
                        KeyCode::Backspace => {
                            search.pop();
                            state.select(Some(0));
                        }
                        KeyCode::Char(c) => {
                            search.push(c);
                            state.select(Some(0));
                        }
                        _ => {}
                    },
                };
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum InputMode {
    Normal,
    Search,
}

fn workspace_detail_lines(ws: &Workspace) -> Vec<Line<'static>> {
    let pane_lines: Vec<Line> = ws
        .panes
        .iter()
        .map(|pane| {
            Line::from(format!(
                "{:<1} {:<14} {:<4} {:<10} {}",
                if pane.active { "*" } else { " " },
                truncate(&pane.window, 14),
                pane.pane,
                truncate(&pane.command, 10),
                pane.path
            ))
        })
        .collect();
    let mut lines = vec![
        Line::from(format!("Name: {}", ws.name)),
        Line::from(format!("ID: {}", ws.id)),
        Line::from(format!("Server: {}", ws.server)),
        Line::from(format!("Session: {}", ws.session)),
        Line::from(format!("Path: {}", ws.root_path)),
        Line::from(format!("Agent: {}", ws.agent)),
        Line::from(format!("Status: {}", ws.status)),
        Line::from(format!("Last seen: {}", ws.last_seen)),
        Line::from(format!(
            "Last attached: {}",
            ws.last_attached_at.as_deref().unwrap_or("never")
        )),
        Line::from(format!("Attach count: {}", ws.attach_count)),
        Line::from(""),
        Line::from("Panes:"),
        Line::from("A window         pane cmd        path"),
    ];
    lines.extend(pane_lines);
    lines.push(Line::from(""));
    lines.push(Line::from(format!("Note: {}", ws.note)));
    lines
}

fn workspace_list_title(show_archived: bool) -> String {
    if show_archived {
        "Workspaces (all)".to_string()
    } else {
        "Workspaces".to_string()
    }
}

fn truncate(value: &str, width: usize) -> String {
    let mut output = String::new();
    for ch in value.chars().take(width) {
        output.push(ch);
    }
    output
}

fn filtered_indices(workspaces: &[Workspace], search: &str, show_archived: bool) -> Vec<usize> {
    let needle = search.trim().to_lowercase();
    workspaces
        .iter()
        .enumerate()
        .filter(|(_, ws)| show_archived || ws.status != "archived")
        .filter(|(_, ws)| needle.is_empty() || workspace_matches(ws, &needle))
        .map(|(index, _)| index)
        .collect()
}

fn workspace_matches(ws: &Workspace, needle: &str) -> bool {
    [
        ws.id.as_str(),
        ws.name.as_str(),
        ws.server.as_str(),
        ws.session.as_str(),
        ws.root_path.as_str(),
        ws.agent.as_str(),
        ws.status.as_str(),
        ws.note.as_str(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(needle))
        || ws.panes.iter().any(|pane| {
            [
                pane.window.as_str(),
                pane.command.as_str(),
                pane.path.as_str(),
                pane.title.as_str(),
            ]
            .iter()
            .any(|value| value.to_lowercase().contains(needle))
        })
}

fn edit_note_from_tui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    workspace: &mut Workspace,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    let result = edit_note(&workspace.id, &workspace.note);
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    // The editor may leave the alternate screen blank while ratatui still
    // believes its previous buffer is current. Force a full repaint.
    terminal.clear()?;

    if let Some(note) = result? {
        let conn = open_db()?;
        migrate(&conn)?;
        conn.execute(
            "update workspaces set note = ?1 where id = ?2",
            params![note, workspace.id],
        )?;
        workspace.note = note;
    }
    Ok(())
}

fn toggle_archive(workspace: &mut Workspace) -> Result<()> {
    let next_status = if workspace.status == "archived" {
        "active"
    } else {
        "archived"
    };
    let conn = open_db()?;
    migrate(&conn)?;
    conn.execute(
        "update workspaces set status = ?1 where id = ?2",
        params![next_status, workspace.id],
    )?;
    workspace.status = next_status.to_string();
    Ok(())
}

fn rescan_from_tui(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<Vec<Workspace>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    let result = scan();
    println!();
    println!("Press Enter to return to ws...");
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    terminal.clear()?;
    result?;

    let conn = open_db()?;
    migrate(&conn)?;
    load_workspaces(&conn)
}

fn edit_note(workspace_id: &str, current_note: &str) -> Result<Option<String>> {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ws-note-{}-{}.md",
        std::process::id(),
        sanitize_filename(workspace_id)
    ));
    {
        let mut file = fs::File::create(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        file.write_all(current_note.as_bytes())?;
    }

    if let Err(err) = edit_file(&path) {
        let _ = fs::remove_file(&path);
        return Err(err);
    }

    let next =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let _ = fs::remove_file(&path);
    if next == current_note {
        Ok(None)
    } else {
        Ok(Some(next))
    }
}

fn edit_file(path: &PathBuf) -> Result<()> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let command = format!("{} {}", editor, shell_quote(&path.to_string_lossy()));
    let status = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to run editor")?;
    if !status.success() {
        bail!("editor exited with status {status}");
    }
    Ok(())
}

fn sanitize_filename(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn scan_server(server: &ServerConfig) -> Result<Vec<(String, Pane)>> {
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

fn remote_session_exists(server: &ServerConfig, session: &str) -> Result<bool> {
    let remote = format!("tmux has-session -t {}", shell_quote(session));
    let output = run_remote(server, &remote)?;
    Ok(output.status.success())
}

fn remote_doctor(server: &ServerConfig) -> Result<DoctorReport> {
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

fn run_remote(server: &ServerConfig, remote: &str) -> Result<std::process::Output> {
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

fn upsert_workspace(conn: &Connection, ws: &Workspace) -> Result<()> {
    conn.execute(
        "insert into workspaces (id, name, server, session, root_path, agent, note, status, last_seen, last_attached_at, attach_count)
         values (?1, ?2, ?3, ?4, ?5, ?6, coalesce((select note from workspaces where id = ?1), ''), coalesce((select status from workspaces where id = ?1), 'active'), ?7, (select last_attached_at from workspaces where id = ?1), coalesce((select attach_count from workspaces where id = ?1), 0))
         on conflict(id) do update set
           name = excluded.name,
           server = excluded.server,
           session = excluded.session,
           root_path = excluded.root_path,
           agent = excluded.agent,
           last_seen = excluded.last_seen",
        params![
            ws.id,
            ws.name,
            ws.server,
            ws.session,
            ws.root_path,
            ws.agent,
            ws.last_seen
        ],
    )?;
    conn.execute("delete from panes where workspace_id = ?1", params![ws.id])?;
    for pane in &ws.panes {
        conn.execute(
            "insert into panes (workspace_id, window, pane, active, command, path, title)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                ws.id,
                pane.window,
                pane.pane,
                pane.active as i64,
                pane.command,
                pane.path,
                pane.title
            ],
        )?;
    }
    Ok(())
}

fn load_workspaces(conn: &Connection) -> Result<Vec<Workspace>> {
    let mut stmt = conn.prepare(
        "select id, name, server, session, root_path, agent, note, status, last_seen, last_attached_at, attach_count
         from workspaces order by coalesce(last_attached_at, last_seen) desc, name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Workspace {
            id: row.get(0)?,
            name: row.get(1)?,
            server: row.get(2)?,
            session: row.get(3)?,
            root_path: row.get(4)?,
            agent: row.get(5)?,
            note: row.get(6)?,
            status: row.get(7)?,
            last_seen: row.get(8)?,
            last_attached_at: row.get(9)?,
            attach_count: row.get(10)?,
            panes: Vec::new(),
        })
    })?;

    let mut workspaces = Vec::new();
    for row in rows {
        let mut ws = row?;
        ws.panes = load_panes(conn, &ws.id)?;
        workspaces.push(ws);
    }
    Ok(workspaces)
}

fn find_workspace(conn: &Connection, name: &str) -> Result<Option<Workspace>> {
    let matches: Vec<Workspace> = load_workspaces(conn)?
        .into_iter()
        .filter(|ws| ws.id == name || ws.name == name || ws.session == name)
        .collect();
    if matches.len() > 1 {
        let ids = matches
            .iter()
            .map(|ws| ws.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        bail!("ambiguous workspace `{name}`; use one of: {ids}");
    }
    Ok(matches.into_iter().next())
}

fn load_panes(conn: &Connection, workspace_id: &str) -> Result<Vec<Pane>> {
    let mut stmt = conn.prepare(
        "select window, pane, active, command, path, title
         from panes where workspace_id = ?1 order by window, pane",
    )?;
    let rows = stmt.query_map(params![workspace_id], |row| {
        Ok(Pane {
            window: row.get(0)?,
            pane: row.get(1)?,
            active: row.get::<_, i64>(2)? == 1,
            command: row.get(3)?,
            path: row.get(4)?,
            title: row.get(5)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn migrate(conn: &Connection) -> Result<()> {
    let old_schema: i64 = conn.query_row(
        "select count(*) from sqlite_master
         where type = 'table'
           and name = 'workspaces'
           and not exists (
             select 1 from pragma_table_info('workspaces') where name = 'id'
           )",
        [],
        |row| row.get(0),
    )?;
    if old_schema > 0 {
        conn.execute_batch(
            "
            drop table if exists panes;
            drop table if exists workspaces;
            ",
        )?;
    }

    conn.execute_batch(
        "
        create table if not exists workspaces (
          id text primary key,
          name text not null,
          server text not null,
          session text not null,
          root_path text not null,
          agent text not null,
          note text not null default '',
          status text not null default 'active',
          last_seen text not null,
          last_attached_at text,
          attach_count integer not null default 0
        );

        create table if not exists panes (
          workspace_id text not null,
          window text not null,
          pane integer not null,
          active integer not null,
          command text not null,
          path text not null,
          title text not null,
          foreign key(workspace_id) references workspaces(id)
        );
        ",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "last_attached_at",
        "alter table workspaces add column last_attached_at text",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "attach_count",
        "alter table workspaces add column attach_count integer not null default 0",
    )?;
    Ok(())
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<()> {
    let sql = format!(
        "select count(*) from pragma_table_info({}) where name = ?1",
        shell_quote(table)
    );
    let exists: i64 = conn.query_row(&sql, params![column], |row| row.get(0))?;
    if exists == 0 {
        conn.execute(alter_sql, [])?;
    }
    Ok(())
}

fn load_or_create_config() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        init_config()?;
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&raw).with_context(|| format!("invalid config {}", path.display()))
}

fn open_db() -> Result<Connection> {
    let path = data_path()?.join("workspaces.db");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Connection::open(path).map_err(Into::into)
}

fn config_path() -> Result<PathBuf> {
    Ok(home_dir()?.join(".config").join("ws").join("config.yaml"))
}

fn data_path() -> Result<PathBuf> {
    Ok(home_dir()?.join(".local").join("share").join("ws"))
}

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("no home directory found")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn tmux_attach_command(session: &str, term: Option<&str>) -> String {
    let term = term.unwrap_or("xterm-256color");
    format!(
        "TERM={} tmux attach -t {}",
        shell_quote(term),
        shell_quote(session)
    )
}

fn attach_ssh_command(ssh: &str) -> String {
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
    use super::{
        Pane, Workspace, attach_ssh_command, filtered_indices, sanitize_filename,
        tmux_attach_command, workspace_matches,
    };

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
    fn search_matches_workspace_metadata_and_panes() {
        let workspace = Workspace {
            id: "server/demo".to_string(),
            name: "demo".to_string(),
            server: "server".to_string(),
            session: "demo".to_string(),
            root_path: "/data/code/demo".to_string(),
            agent: "codex".to_string(),
            panes: vec![Pane {
                window: "0:codex".to_string(),
                pane: 0,
                active: true,
                command: "bash".to_string(),
                path: "/data/code/demo/frontend".to_string(),
                title: "frontend work".to_string(),
            }],
            note: "uses uv".to_string(),
            status: "active".to_string(),
            last_seen: "now".to_string(),
            last_attached_at: None,
            attach_count: 0,
        };

        assert!(workspace_matches(&workspace, "frontend"));
        assert!(workspace_matches(&workspace, "uv"));
        assert!(!workspace_matches(&workspace, "missing"));
    }

    #[test]
    fn sanitizes_workspace_id_for_temp_filename() {
        assert_eq!(
            sanitize_filename("AI-Teacher-Baidu/NeuroPlay"),
            "AI-Teacher-Baidu_NeuroPlay"
        );
    }

    #[test]
    fn archived_workspaces_are_hidden_until_requested() {
        let active = test_workspace("server/active", "active");
        let archived = test_workspace("server/archived", "archived");
        let workspaces = vec![active, archived];

        assert_eq!(filtered_indices(&workspaces, "", false), vec![0]);
        assert_eq!(filtered_indices(&workspaces, "", true), vec![0, 1]);
    }

    #[test]
    fn workspace_root_prefers_agent_pane_path() {
        let workspaces = super::group_panes(
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

    fn test_workspace(id: &str, status: &str) -> Workspace {
        Workspace {
            id: id.to_string(),
            name: id.to_string(),
            server: "server".to_string(),
            session: id.to_string(),
            root_path: "/tmp".to_string(),
            agent: "bash".to_string(),
            panes: Vec::new(),
            note: String::new(),
            status: status.to_string(),
            last_seen: "now".to_string(),
            last_attached_at: None,
            attach_count: 0,
        }
    }
}
