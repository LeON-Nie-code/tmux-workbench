use std::{
    io::{self, Stdout},
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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use rusqlite::params;

use crate::{
    commands::{attach, refresh_index, scan},
    db::{load_workspaces, migrate, open_db},
    model::Workspace,
    util::{edit_note, truncate},
};

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

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
    show_archived: bool,
) -> Option<String> {
    let selected = state.selected()?;
    let filtered = filtered_indices(workspaces, search, show_archived);
    filtered
        .get(selected)
        .map(|index| workspaces[*index].id.clone())
}

fn restore_selection(
    workspaces: &[Workspace],
    state: &mut ListState,
    selected_id: Option<&str>,
    search: &str,
    show_archived: bool,
) {
    let filtered = filtered_indices(workspaces, search, show_archived);
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
    let mut show_archived = false;
    let mut last_auto_refresh = Instant::now();

    loop {
        if last_auto_refresh.elapsed() >= AUTO_REFRESH_INTERVAL {
            let selected_id = selected_workspace_id(&workspaces, &state, &search, show_archived);
            if refresh_index().is_ok() {
                workspaces = load_indexed_workspaces()?;
                restore_selection(
                    &workspaces,
                    &mut state,
                    selected_id.as_deref(),
                    &search,
                    show_archived,
                );
            }
            last_auto_refresh = Instant::now();
        }

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
                            format!("{:<22}", truncate(display_name(ws), 22)),
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
                            let selected_id =
                                selected_workspace_id(&workspaces, &state, &search, show_archived);
                            workspaces = rescan_from_tui(terminal)?;
                            restore_selection(
                                &workspaces,
                                &mut state,
                                selected_id.as_deref(),
                                &search,
                                show_archived,
                            );
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
        Line::from(format!("Alias: {}", ws.alias.as_deref().unwrap_or(""))),
        Line::from(format!("ID: {}", ws.id)),
        Line::from(format!("Server: {}", ws.server)),
        Line::from(format!("Session: {}", ws.session)),
        Line::from(format!("Path: {}", ws.root_path)),
        Line::from(format!("Agent: {}", ws.agent)),
        Line::from(format!("Status: {}", ws.status)),
        Line::from(format!("Tags: {}", ws.tags.join(", "))),
        Line::from(format!("Git: {}", git_detail(ws))),
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

fn workspace_list_title(show_archived: bool) -> String {
    if show_archived {
        "Workspaces (all)".to_string()
    } else {
        "Workspaces".to_string()
    }
}

fn display_name(ws: &Workspace) -> &str {
    ws.alias.as_deref().unwrap_or(&ws.name)
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
        ws.alias.as_deref().unwrap_or(""),
        ws.server.as_str(),
        ws.session.as_str(),
        ws.root_path.as_str(),
        ws.agent.as_str(),
        ws.status.as_str(),
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

    use super::{filtered_indices, workspace_matches};

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
    }

    #[test]
    fn archived_workspaces_are_hidden_until_requested() {
        let active = test_workspace("server/active", "active");
        let archived = test_workspace("server/archived", "archived");
        let workspaces = vec![active, archived];

        assert_eq!(filtered_indices(&workspaces, "", false), vec![0]);
        assert_eq!(filtered_indices(&workspaces, "", true), vec![0, 1]);
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
            tags: Vec::new(),
            last_seen: "now".to_string(),
            last_attached_at: None,
            attach_count: 0,
            git: None,
        }
    }
}
