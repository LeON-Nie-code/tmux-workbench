use std::{
    io::{self, Stdout},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use rusqlite::params;

use crate::{
    commands::{ScanSummary, attach, refresh_index_report, scan},
    db::{load_workspaces, migrate, open_db},
    model::Workspace,
    util::{edit_note, truncate},
};

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

type RefreshResult = std::result::Result<RefreshPayload, String>;

#[derive(Debug)]
struct RefreshPayload {
    workspaces: Vec<Workspace>,
    summary: ScanSummary,
}

pub fn run_tui() -> Result<()> {
    let workspaces = load_indexed_workspaces()?;
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

fn load_indexed_workspaces() -> Result<Vec<Workspace>> {
    let conn = open_db()?;
    migrate(&conn)?;
    load_workspaces(&conn)
}

fn selected_workspace_id(
    workspaces: &[Workspace],
    state: &ListState,
    search: &str,
    view: WorkspaceView,
    server_filter: Option<&str>,
) -> Option<String> {
    let selected = state.selected()?;
    let filtered = filtered_indices(workspaces, search, view, server_filter);
    filtered
        .get(selected)
        .map(|index| workspaces[*index].id.clone())
}

fn restore_selection(
    workspaces: &[Workspace],
    state: &mut ListState,
    selected_id: Option<&str>,
    search: &str,
    view: WorkspaceView,
    server_filter: Option<&str>,
) {
    let filtered = filtered_indices(workspaces, search, view, server_filter);
    if filtered.is_empty() {
        state.select(None);
        return;
    }
    let selected = selected_id
        .and_then(|id| {
            filtered
                .iter()
                .position(|index| workspaces[*index].id == id)
        })
        .unwrap_or(0);
    state.select(Some(selected));
}

fn draw_tui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut workspaces: Vec<Workspace>,
) -> Result<()> {
    let mut state = ListState::default();
    state.select(Some(0));
    let mut search = String::new();
    let mut mode = InputMode::Normal;
    let mut view = WorkspaceView::All;
    let mut server_filter: Option<String> = None;
    let mut last_auto_refresh = Instant::now();
    let mut auto_refresh_in_flight = false;
    let mut scan_status = String::from("Scan: idle");
    let (refresh_tx, refresh_rx) = mpsc::channel();

    loop {
        apply_completed_refresh(
            &refresh_rx,
            &mut auto_refresh_in_flight,
            &mut workspaces,
            &mut state,
            &search,
            view,
            server_filter.as_deref(),
            &mut scan_status,
        );

        if last_auto_refresh.elapsed() >= AUTO_REFRESH_INTERVAL && !auto_refresh_in_flight {
            spawn_auto_refresh(refresh_tx.clone());
            auto_refresh_in_flight = true;
            scan_status = "Scan: refreshing...".to_string();
            last_auto_refresh = Instant::now();
        }

        let filtered = filtered_indices(&workspaces, &search, view, server_filter.as_deref());
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
                InputMode::Normal if search.is_empty() => {
                    workspace_list_title(view, server_filter.as_deref())
                }
                InputMode::Normal => {
                    format!(
                        "{}  /{search}",
                        workspace_list_title(view, server_filter.as_deref())
                    )
                }
                InputMode::Search => format!("Search  /{search}"),
            };
            let items: Vec<ListItem> = filtered
                .iter()
                .map(|index| &workspaces[*index])
                .map(|ws| {
                    let style = if ws.status == "archived" {
                        Style::default().add_modifier(Modifier::DIM)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{:<22}", truncate(display_name(ws), 22)),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!("{:<20}", truncate(&ws.server, 20))),
                        Span::raw(format!("{:<8}", truncate(&ws.agent, 8))),
                        Span::raw(format!("{:<12}", truncate(&workspace_state(ws), 12))),
                    ]))
                    .style(style)
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
            lines.push(Line::from(scan_status.clone()));
            lines.push(Line::from(match mode {
                InputMode::Normal => controls_line(view),
                InputMode::Search => "Type to search  Enter accept  Esc clear",
            }));

            let detail = Paragraph::new(lines)
                .block(Block::default().title("Detail").borders(Borders::ALL))
                .wrap(Wrap { trim: false });
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
                            }
                        }
                        KeyCode::Char('z') => {
                            let selected_id = selected_workspace_id(
                                &workspaces,
                                &state,
                                &search,
                                view,
                                server_filter.as_deref(),
                            );
                            view = if view == WorkspaceView::Archived {
                                WorkspaceView::All
                            } else {
                                WorkspaceView::Archived
                            };
                            restore_selection(
                                &workspaces,
                                &mut state,
                                selected_id.as_deref(),
                                &search,
                                view,
                                server_filter.as_deref(),
                            );
                        }
                        KeyCode::Char('v') => {
                            let selected_id = selected_workspace_id(
                                &workspaces,
                                &state,
                                &search,
                                view,
                                server_filter.as_deref(),
                            );
                            view = view.next();
                            restore_selection(
                                &workspaces,
                                &mut state,
                                selected_id.as_deref(),
                                &search,
                                view,
                                server_filter.as_deref(),
                            );
                        }
                        KeyCode::Char('s') => {
                            let selected_id = selected_workspace_id(
                                &workspaces,
                                &state,
                                &search,
                                view,
                                server_filter.as_deref(),
                            );
                            server_filter =
                                next_server_filter(&workspaces, server_filter.as_deref());
                            restore_selection(
                                &workspaces,
                                &mut state,
                                selected_id.as_deref(),
                                &search,
                                view,
                                server_filter.as_deref(),
                            );
                        }
                        KeyCode::Char('r') => {
                            let selected_id = selected_workspace_id(
                                &workspaces,
                                &state,
                                &search,
                                view,
                                server_filter.as_deref(),
                            );
                            workspaces = rescan_from_tui(terminal)?;
                            restore_selection(
                                &workspaces,
                                &mut state,
                                selected_id.as_deref(),
                                &search,
                                view,
                                server_filter.as_deref(),
                            );
                            scan_status = "Scan: manual refresh complete".to_string();
                            last_auto_refresh = Instant::now();
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

fn apply_completed_refresh(
    refresh_rx: &Receiver<RefreshResult>,
    auto_refresh_in_flight: &mut bool,
    workspaces: &mut Vec<Workspace>,
    state: &mut ListState,
    search: &str,
    view: WorkspaceView,
    server_filter: Option<&str>,
    scan_status: &mut String,
) {
    while let Ok(result) = refresh_rx.try_recv() {
        *auto_refresh_in_flight = false;
        match result {
            Ok(payload) => {
                let selected_id =
                    selected_workspace_id(&workspaces, &state, &search, view, server_filter);
                *workspaces = payload.workspaces;
                restore_selection(
                    workspaces,
                    state,
                    selected_id.as_deref(),
                    search,
                    view,
                    server_filter,
                );
                *scan_status = scan_status_from_summary(&payload.summary);
            }
            Err(err) => {
                *scan_status = format!("Scan: failed ({err})");
            }
        }
    }
}

fn spawn_auto_refresh(refresh_tx: Sender<RefreshResult>) {
    thread::spawn(move || {
        let result = refresh_index_report()
            .and_then(|summary| {
                let workspaces = load_indexed_workspaces()?;
                Ok(RefreshPayload {
                    workspaces,
                    summary,
                })
            })
            .map_err(|err| format!("{err:#}"));
        let _ = refresh_tx.send(result);
    });
}

fn scan_status_from_summary(summary: &ScanSummary) -> String {
    if summary.errors.is_empty() {
        format!("Scan: ok, {} workspaces", summary.total)
    } else {
        format!(
            "Scan: {} workspaces, {} server error(s): {}",
            summary.total,
            summary.errors.len(),
            summary.errors.join("; ")
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum InputMode {
    Normal,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceView {
    All,
    Active,
    Archived,
}

impl WorkspaceView {
    fn next(self) -> Self {
        match self {
            WorkspaceView::All => WorkspaceView::Active,
            WorkspaceView::Active => WorkspaceView::Archived,
            WorkspaceView::Archived => WorkspaceView::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            WorkspaceView::All => "all",
            WorkspaceView::Active => "active",
            WorkspaceView::Archived => "archived",
        }
    }
}

fn workspace_detail_lines(ws: &Workspace) -> Vec<Line<'static>> {
    let active_command = ws
        .panes
        .iter()
        .find(|pane| pane.active)
        .map(|pane| format!("{} in {}", pane.command, pane.path))
        .unwrap_or_else(|| "-".to_string());
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
        Line::from(format!("Alias: {}", ws.alias.as_deref().unwrap_or(""))),
        Line::from(format!("ID: {}", ws.id)),
        Line::from(format!("Server: {}", ws.server)),
        Line::from(format!("Session: {}", ws.session)),
        Line::from(format!("Path: {}", ws.root_path)),
        Line::from(format!("Agent: {}", ws.agent)),
        Line::from(format!("Panes: {}", ws.panes.len())),
        Line::from(format!("Active: {}", active_command)),
        Line::from(format!("Status: {}", ws.status)),
        Line::from(format!("Presence: {}", ws.presence)),
        Line::from(format!("Tags: {}", ws.tags.join(", "))),
        Line::from(format!("Git: {}", git_detail(ws))),
        Line::from(format!("Last seen: {}", ws.last_seen)),
        Line::from(format!(
            "Last attached: {}",
            ws.last_attached_at.as_deref().unwrap_or("never")
        )),
        Line::from(format!("Attach count: {}", ws.attach_count)),
        Line::from(""),
        Line::from("Pane detail:"),
        Line::from("A window         pane cmd        path"),
    ];
    lines.extend(pane_lines);
    lines.push(Line::from(""));
    lines.extend(note_lines(&ws.note));
    lines
}

fn note_lines(note: &str) -> Vec<Line<'static>> {
    if note.is_empty() {
        return vec![Line::from("Note:")];
    }

    let mut lines = vec![Line::from("Note:")];
    lines.extend(note.lines().map(|line| Line::from(format!("  {line}"))));
    if note.ends_with('\n') {
        lines.push(Line::from("  "));
    }
    lines
}

fn git_detail(ws: &Workspace) -> String {
    let Some(git) = &ws.git else {
        return "not a git repo".to_string();
    };
    let branch = git.branch.as_deref().unwrap_or("detached");
    let head = git.head.as_deref().unwrap_or("unknown");
    let dirty = if git.dirty { "dirty" } else { "clean" };
    let remote = git.remote.as_deref().unwrap_or("no remote");
    format!(
        "{branch} @ {head} ({dirty}, ahead {}, behind {}) {remote}",
        git.ahead, git.behind
    )
}

fn workspace_list_title(view: WorkspaceView, server_filter: Option<&str>) -> String {
    format!(
        "Workspaces ({}, {})",
        view.label(),
        server_filter.unwrap_or("all servers")
    )
}

fn controls_line(view: WorkspaceView) -> &'static str {
    match view {
        WorkspaceView::Archived => {
            "Enter attach  / search  n note  a unarchive  v view  s server  z all  r rescan  q quit"
        }
        _ => {
            "Enter attach  / search  n note  a archive  v view  s server  z archived  r rescan  q quit"
        }
    }
}

fn display_name(ws: &Workspace) -> &str {
    ws.alias.as_deref().unwrap_or(&ws.name)
}

fn filtered_indices(
    workspaces: &[Workspace],
    search: &str,
    view: WorkspaceView,
    server_filter: Option<&str>,
) -> Vec<usize> {
    let query = SearchQuery::parse(search);
    workspaces
        .iter()
        .enumerate()
        .filter(|(_, ws)| workspace_in_view(ws, view))
        .filter(|(_, ws)| server_filter.is_none_or(|server| ws.server == server))
        .filter(|(_, ws)| query.matches(ws))
        .map(|(index, _)| index)
        .collect()
}

fn workspace_in_view(ws: &Workspace, view: WorkspaceView) -> bool {
    match view {
        WorkspaceView::All => true,
        WorkspaceView::Active => ws.status != "archived",
        WorkspaceView::Archived => ws.status == "archived",
    }
}

#[derive(Debug, Default)]
struct SearchQuery {
    text: Vec<String>,
    server: Vec<String>,
    status: Vec<String>,
    tag: Vec<String>,
    git: Vec<String>,
}

impl SearchQuery {
    fn parse(search: &str) -> Self {
        let mut query = SearchQuery::default();
        for token in search.split_whitespace() {
            let token = token.to_lowercase();
            if let Some(value) = token.strip_prefix("server:") {
                query.server.push(value.to_string());
            } else if let Some(value) = token.strip_prefix("status:") {
                query.status.push(value.to_string());
            } else if let Some(value) = token.strip_prefix("tag:") {
                query.tag.push(value.to_string());
            } else if let Some(value) = token.strip_prefix("git:") {
                query.git.push(value.to_string());
            } else if !token.is_empty() {
                query.text.push(token);
            }
        }
        query
    }

    fn matches(&self, ws: &Workspace) -> bool {
        self.server
            .iter()
            .all(|value| ws.server.to_lowercase().contains(value))
            && self
                .status
                .iter()
                .all(|value| ws.status.to_lowercase().contains(value))
            && self
                .tag
                .iter()
                .all(|value| ws.tags.iter().any(|tag| tag.to_lowercase().contains(value)))
            && self.git.iter().all(|value| git_matches(ws, value))
            && self.text.iter().all(|value| workspace_matches(ws, value))
    }
}

fn workspace_matches(ws: &Workspace, needle: &str) -> bool {
    let needle = &needle.to_lowercase();
    [
        ws.id.as_str(),
        ws.name.as_str(),
        ws.alias.as_deref().unwrap_or(""),
        ws.server.as_str(),
        ws.session.as_str(),
        ws.root_path.as_str(),
        ws.agent.as_str(),
        ws.status.as_str(),
        ws.presence.as_str(),
        ws.note.as_str(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(needle))
        || ws
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(needle))
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

fn git_matches(ws: &Workspace, needle: &str) -> bool {
    let Some(git) = &ws.git else {
        return needle == "none";
    };
    match needle {
        "dirty" => git.dirty,
        "clean" => !git.dirty,
        "remote" => git.remote.is_some(),
        "ahead" => git.ahead > 0,
        "behind" => git.behind > 0,
        value => [
            git.branch.as_deref().unwrap_or(""),
            git.head.as_deref().unwrap_or(""),
            git.remote.as_deref().unwrap_or(""),
        ]
        .iter()
        .any(|field| field.to_lowercase().contains(value)),
    }
}

fn workspace_state(ws: &Workspace) -> String {
    if ws.presence == "seen" {
        ws.status.clone()
    } else {
        format!("{}/{}", ws.status, ws.presence)
    }
}

fn next_server_filter(workspaces: &[Workspace], current: Option<&str>) -> Option<String> {
    let mut servers = workspaces
        .iter()
        .map(|ws| ws.server.as_str())
        .collect::<Vec<_>>();
    servers.sort_unstable();
    servers.dedup();
    if servers.is_empty() {
        return None;
    }

    let next = current
        .and_then(|server| servers.iter().position(|item| *item == server))
        .map_or(0, |index| index + 1);
    servers.get(next).map(|server| (*server).to_string())
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

#[cfg(test)]
mod tests {
    use crate::model::{Pane, Workspace};

    use super::{SearchQuery, WorkspaceView, filtered_indices, note_lines, workspace_matches};

    #[test]
    fn search_matches_workspace_metadata_and_panes() {
        let workspace = Workspace {
            id: "server/demo".to_string(),
            name: "demo".to_string(),
            alias: Some("demo-alias".to_string()),
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
            presence: "seen".to_string(),
            tags: vec!["research".to_string()],
            last_seen: "now".to_string(),
            last_attached_at: None,
            attach_count: 0,
            git: None,
        };

        assert!(workspace_matches(&workspace, "frontend"));
        assert!(workspace_matches(&workspace, "uv"));
        assert!(workspace_matches(&workspace, "demo-alias"));
        assert!(workspace_matches(&workspace, "research"));
        assert!(!workspace_matches(&workspace, "missing"));

        assert!(SearchQuery::parse("server:serv status:active tag:research").matches(&workspace));
        assert!(!SearchQuery::parse("server:other").matches(&workspace));
    }

    #[test]
    fn workspace_view_filters_status() {
        let active = test_workspace("server/active", "active");
        let archived = test_workspace("server/archived", "archived");
        let workspaces = vec![active, archived];

        assert_eq!(
            filtered_indices(&workspaces, "", WorkspaceView::Active, None),
            vec![0]
        );
        assert_eq!(
            filtered_indices(&workspaces, "", WorkspaceView::All, None),
            vec![0, 1]
        );
        assert_eq!(
            filtered_indices(&workspaces, "", WorkspaceView::Archived, None),
            vec![1]
        );
        assert_eq!(
            filtered_indices(&workspaces, "", WorkspaceView::All, Some("server")),
            vec![0, 1]
        );
    }

    #[test]
    fn note_lines_preserve_newlines() {
        let lines = note_lines("first\n\nsecond");
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].to_string(), "Note:");
        assert_eq!(lines[1].to_string(), "  first");
        assert_eq!(lines[2].to_string(), "  ");
        assert_eq!(lines[3].to_string(), "  second");
    }

    fn test_workspace(id: &str, status: &str) -> Workspace {
        Workspace {
            id: id.to_string(),
            name: id.to_string(),
            alias: None,
            server: "server".to_string(),
            session: id.to_string(),
            root_path: "/tmp".to_string(),
            agent: "bash".to_string(),
            panes: Vec::new(),
            note: String::new(),
            status: status.to_string(),
            presence: "seen".to_string(),
            tags: Vec::new(),
            last_seen: "now".to_string(),
            last_attached_at: None,
            attach_count: 0,
            git: None,
        }
    }
}
