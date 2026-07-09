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
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use rusqlite::params;

use crate::{
    commands::{
        ScanSummary, attach_command_for_workspace, refresh_index_report, run_attach_command, scan,
    },
    db::{load_workspaces, migrate, open_db},
    model::Workspace,
    util::{edit_note, truncate},
};

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

type RefreshResult = std::result::Result<RefreshPayload, String>;
type AttachResult = std::result::Result<String, String>;

#[derive(Debug)]
struct RefreshPayload {
    workspaces: Vec<Workspace>,
    summary: ScanSummary,
}

#[derive(Debug)]
struct AttachState {
    workspace_id: String,
    started_at: Instant,
    rx: Receiver<AttachResult>,
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
    let mut attach_state: Option<AttachState> = None;
    let mut scan_status = String::from("Scan: idle");
    let (refresh_tx, refresh_rx) = mpsc::channel();

    loop {
        apply_completed_refresh(
            &refresh_rx,
            RefreshApply {
                auto_refresh_in_flight: &mut auto_refresh_in_flight,
                workspaces: &mut workspaces,
                state: &mut state,
                scan_status: &mut scan_status,
            },
            &search,
            view,
            server_filter.as_deref(),
        );

        if let Some(state) = &attach_state {
            match state.rx.try_recv() {
                Ok(Ok(command)) => {
                    disable_raw_mode()?;
                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                    return run_attach_command(&command);
                }
                Ok(Err(err)) => {
                    scan_status = format!("Attach failed: {err}");
                    attach_state = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    scan_status = "Attach failed: worker disconnected".to_string();
                    attach_state = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        if attach_state.is_none()
            && last_auto_refresh.elapsed() >= AUTO_REFRESH_INTERVAL
            && !auto_refresh_in_flight
        {
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
            let shell = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(8),
                    Constraint::Length(1),
                ])
                .split(frame.area());

            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
                .split(shell[1]);

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

            let header = app_header(
                &workspaces,
                filtered.len(),
                view,
                server_filter.as_deref(),
                &search,
                &scan_status,
                mode,
            );
            frame.render_widget(header, shell[0]);

            frame.render_widget(panel_block(title.clone()), chunks[0]);
            let left = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(inner(chunks[0]));
            frame.render_widget(list_header(), left[0]);

            let items: Vec<ListItem> = filtered
                .iter()
                .map(|index| &workspaces[*index])
                .map(workspace_list_item)
                .collect();
            let list = List::new(items)
                .highlight_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Rgb(94, 234, 212))
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED),
                )
                .highlight_symbol("▌ ");
            frame.render_stateful_widget(list, left[1], &mut state);

            let lines = if let Some(selected) = state.selected() {
                let ws = &workspaces[filtered[selected]];
                workspace_detail_lines(ws)
            } else {
                vec![Line::from("No matching workspaces")]
            };

            let detail = Paragraph::new(lines)
                .block(panel_block("Workspace"))
                .wrap(Wrap { trim: false });
            frame.render_widget(detail, chunks[1]);

            frame.render_widget(footer(mode, view), shell[2]);

            if let Some(state) = &attach_state {
                render_attach_loading(frame, frame.area(), state);
            }
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if attach_state.is_some() {
                    continue;
                }
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
                        KeyCode::Char('j') | KeyCode::Down if !filtered.is_empty() => {
                            let next = state
                                .selected()
                                .unwrap_or(0)
                                .saturating_add(1)
                                .min(filtered.len() - 1);
                            state.select(Some(next));
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            let prev = state.selected().unwrap_or(0).saturating_sub(1);
                            state.select(Some(prev));
                        }
                        KeyCode::Enter => {
                            if let Some(selected) = state.selected() {
                                let ws = &workspaces[filtered[selected]];
                                attach_state = Some(spawn_attach(ws.id.clone()));
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

fn app_header(
    workspaces: &[Workspace],
    filtered_count: usize,
    view: WorkspaceView,
    server_filter: Option<&str>,
    search: &str,
    scan_status: &str,
    mode: InputMode,
) -> Paragraph<'static> {
    let total = workspaces.len();
    let active = workspaces
        .iter()
        .filter(|workspace| workspace.status != "archived")
        .count();
    let missing = workspaces
        .iter()
        .filter(|workspace| workspace.presence != "seen")
        .count();
    let search_label = if search.is_empty() {
        "none".to_string()
    } else {
        search.to_string()
    };
    let mode_label = match mode {
        InputMode::Normal => "normal",
        InputMode::Search => "search",
    };

    Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                " ws ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(94, 234, 212))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                "Tmux Workbench",
                Style::default()
                    .fg(Color::Rgb(226, 232, 240))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{} shown / {} total", filtered_count, total),
                Style::default().fg(Color::Rgb(148, 163, 184)),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{} active", active),
                Style::default().fg(Color::Rgb(134, 239, 172)),
            ),
            Span::raw("  "),
            Span::styled(format!("{} missing", missing), presence_style("missing")),
        ]),
        Line::from(vec![
            label_span("view"),
            value_span(view.label()),
            label_span("server"),
            value_span(server_filter.unwrap_or("all")),
            label_span("mode"),
            value_span(mode_label),
            label_span("search"),
            value_span(&search_label),
            label_span("scan"),
            Span::styled(scan_status.to_string(), scan_style(scan_status)),
        ]),
    ])
    .block(Block::default().style(Style::default().bg(Color::Rgb(15, 23, 42))))
}

fn spawn_attach(workspace_id: String) -> AttachState {
    let (tx, rx) = mpsc::channel();
    let worker_workspace_id = workspace_id.clone();
    thread::spawn(move || {
        let result =
            attach_command_for_workspace(&worker_workspace_id).map_err(|err| format!("{err:#}"));
        let _ = tx.send(result);
    });
    AttachState {
        workspace_id,
        started_at: Instant::now(),
        rx,
    }
}

fn render_attach_loading(frame: &mut Frame<'_>, area: Rect, state: &AttachState) {
    let popup = centered_rect(56, 9, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(panel_block("Attaching"), popup);

    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let frame_index = (state.started_at.elapsed().as_millis() / 120) as usize % spinner.len();
    let elapsed = state.started_at.elapsed().as_secs_f32();
    let inner = inner(popup);
    let lines = vec![
        Line::from(vec![
            Span::styled(
                spinner[frame_index],
                Style::default()
                    .fg(Color::Rgb(94, 234, 212))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                "Preparing tmux attach",
                Style::default()
                    .fg(Color::Rgb(226, 232, 240))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "workspace  ",
                Style::default().fg(Color::Rgb(100, 116, 139)),
            ),
            Span::styled(
                state.workspace_id.clone(),
                Style::default().fg(Color::Rgb(226, 232, 240)),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "status     ",
                Style::default().fg(Color::Rgb(100, 116, 139)),
            ),
            Span::styled(
                "checking session and opening shell",
                Style::default().fg(Color::Rgb(148, 163, 184)),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "elapsed    ",
                Style::default().fg(Color::Rgb(100, 116, 139)),
            ),
            Span::styled(
                format!("{elapsed:.1}s"),
                Style::default().fg(Color::Rgb(134, 239, 172)),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn footer(mode: InputMode, view: WorkspaceView) -> Paragraph<'static> {
    let text = match mode {
        InputMode::Normal => controls_line(view),
        InputMode::Search => "Type to search | Enter accept | Esc clear",
    };
    Paragraph::new(Line::from(vec![
        Span::styled(
            " keys ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(94, 234, 212))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(text, Style::default().fg(Color::Rgb(148, 163, 184))),
    ]))
}

fn panel_block(title: impl Into<String>) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            format!(" {} ", title.into()),
            Style::default()
                .fg(Color::Rgb(125, 211, 252))
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(71, 85, 105)))
}

fn inner(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn list_header() -> Paragraph<'static> {
    Paragraph::new(Line::from(vec![
        Span::styled(format!("{:<24}", "WORKSPACE"), header_style()),
        Span::styled(format!("{:<20}", "SERVER"), header_style()),
        Span::styled(format!("{:<8}", "AGENT"), header_style()),
        Span::styled("STATE", header_style()),
    ]))
}

fn workspace_list_item(ws: &Workspace) -> ListItem<'static> {
    let base_style = if ws.status == "archived" {
        Style::default().fg(Color::Rgb(100, 116, 139))
    } else {
        Style::default().fg(Color::Rgb(226, 232, 240))
    };
    ListItem::new(Line::from(vec![
        Span::styled(
            format!("{:<24}", truncate(display_name(ws), 22)),
            base_style.add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<20}", truncate(&ws.server, 20)),
            Style::default().fg(Color::Rgb(148, 163, 184)),
        ),
        Span::styled(
            format!("{:<8}", truncate(&ws.agent, 8)),
            Style::default().fg(Color::Rgb(253, 224, 71)),
        ),
        Span::styled(
            format!("{:<12}", truncate(&workspace_state(ws), 12)),
            status_style(ws),
        ),
    ]))
}

fn label_span(label: &str) -> Span<'static> {
    Span::styled(
        format!(" {} ", label),
        Style::default().fg(Color::Rgb(100, 116, 139)),
    )
}

fn value_span(value: &str) -> Span<'static> {
    Span::styled(
        format!("{}  ", value),
        Style::default().fg(Color::Rgb(226, 232, 240)),
    )
}

fn header_style() -> Style {
    Style::default()
        .fg(Color::Rgb(100, 116, 139))
        .add_modifier(Modifier::BOLD)
}

fn status_style(ws: &Workspace) -> Style {
    if ws.presence != "seen" {
        return presence_style(&ws.presence);
    }
    match ws.status.as_str() {
        "archived" => Style::default().fg(Color::Rgb(100, 116, 139)),
        "active" => Style::default().fg(Color::Rgb(134, 239, 172)),
        _ => Style::default().fg(Color::Rgb(253, 224, 71)),
    }
}

fn presence_style(presence: &str) -> Style {
    match presence {
        "seen" => Style::default().fg(Color::Rgb(134, 239, 172)),
        "missing" => Style::default().fg(Color::Rgb(248, 113, 113)),
        _ => Style::default().fg(Color::Rgb(253, 224, 71)),
    }
}

fn scan_style(scan_status: &str) -> Style {
    if scan_status.contains("failed") || scan_status.contains("error") {
        Style::default().fg(Color::Rgb(248, 113, 113))
    } else if scan_status.contains("refreshing") {
        Style::default().fg(Color::Rgb(253, 224, 71))
    } else {
        Style::default().fg(Color::Rgb(134, 239, 172))
    }
}

struct RefreshApply<'a> {
    auto_refresh_in_flight: &'a mut bool,
    workspaces: &'a mut Vec<Workspace>,
    state: &'a mut ListState,
    scan_status: &'a mut String,
}

fn apply_completed_refresh(
    refresh_rx: &Receiver<RefreshResult>,
    apply: RefreshApply<'_>,
    search: &str,
    view: WorkspaceView,
    server_filter: Option<&str>,
) {
    while let Ok(result) = refresh_rx.try_recv() {
        *apply.auto_refresh_in_flight = false;
        match result {
            Ok(payload) => {
                let selected_id = selected_workspace_id(
                    apply.workspaces,
                    apply.state,
                    search,
                    view,
                    server_filter,
                );
                *apply.workspaces = payload.workspaces;
                restore_selection(
                    apply.workspaces,
                    apply.state,
                    selected_id.as_deref(),
                    search,
                    view,
                    server_filter,
                );
                *apply.scan_status = scan_status_from_summary(&payload.summary);
            }
            Err(err) => {
                *apply.scan_status = format!("Scan: failed ({err})");
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
            let marker = if pane.active { "*" } else { " " };
            Line::from(vec![
                Span::styled(
                    format!("{:<1} ", marker),
                    Style::default().fg(Color::Rgb(125, 211, 252)),
                ),
                Span::styled(
                    format!("{:<14}", truncate(&pane.window, 14)),
                    Style::default().fg(Color::Rgb(226, 232, 240)),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:<4}", pane.pane),
                    Style::default().fg(Color::Rgb(100, 116, 139)),
                ),
                Span::styled(
                    format!("{:<10}", truncate(&pane.command, 10)),
                    Style::default().fg(Color::Rgb(253, 224, 71)),
                ),
                Span::styled(
                    pane.path.clone(),
                    Style::default().fg(Color::Rgb(148, 163, 184)),
                ),
            ])
        })
        .collect();
    let mut lines = vec![
        section_line("Overview"),
        field_line("Name", display_name(ws)),
        field_line("ID", &ws.id),
        field_line("Server", &ws.server),
        field_line("Session", &ws.session),
        field_line("Path", &ws.root_path),
        field_line("Tags", &tags_detail(ws)),
        Line::from(""),
        section_line("Runtime"),
        field_line("Agent", &ws.agent),
        field_line("Agent docs", &agent_context_detail(ws)),
        field_line("Panes", &ws.panes.len().to_string()),
        field_line("Active", &active_command),
        field_line("Status", &workspace_state(ws)),
        Line::from(""),
        section_line("Git"),
        field_line("State", &git_detail(ws)),
        Line::from(""),
        section_line("Activity"),
        field_line("Last seen", &ws.last_seen),
        field_line(
            "Last attached",
            ws.last_attached_at.as_deref().unwrap_or("never"),
        ),
        field_line("Attach count", &ws.attach_count.to_string()),
        Line::from(""),
        section_line("Panes"),
        Line::from(vec![
            Span::styled("A ", header_style()),
            Span::styled(format!("{:<14} ", "WINDOW"), header_style()),
            Span::styled(format!("{:<5}", "PANE"), header_style()),
            Span::styled(format!("{:<10}", "CMD"), header_style()),
            Span::styled("PATH", header_style()),
        ]),
    ];
    lines.extend(pane_lines);
    lines.push(Line::from(""));
    lines.push(section_line("Note"));
    lines.extend(note_lines(&ws.note));
    lines
}

fn section_line(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(Color::Rgb(125, 211, 252))
            .add_modifier(Modifier::BOLD),
    ))
}

fn field_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{:<15}", label),
            Style::default().fg(Color::Rgb(100, 116, 139)),
        ),
        Span::styled(
            value.to_string(),
            Style::default().fg(Color::Rgb(226, 232, 240)),
        ),
    ])
}

fn tags_detail(ws: &Workspace) -> String {
    if ws.tags.is_empty() {
        "-".to_string()
    } else {
        ws.tags.join(", ")
    }
}

fn agent_context_detail(ws: &Workspace) -> String {
    if ws.agent_context.is_empty() {
        "-".to_string()
    } else {
        ws.agent_context
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn note_lines(note: &str) -> Vec<Line<'static>> {
    if note.is_empty() {
        return vec![field_line("", "-")];
    }

    let mut lines = Vec::new();
    lines.extend(note.lines().map(|line| {
        Line::from(Span::styled(
            format!("  {line}"),
            Style::default().fg(Color::Rgb(226, 232, 240)),
        ))
    }));
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
        || ws.agent_context.iter().any(|file| {
            [
                file.path.as_str(),
                file.title.as_str(),
                file.preview.as_str(),
            ]
            .iter()
            .any(|value| value.to_lowercase().contains(needle))
        })
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
    use crate::model::{AgentContextFile, Pane, Workspace};

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
            agent_context: vec![AgentContextFile {
                path: "AGENTS.md".to_string(),
                title: "Agent guide".to_string(),
                preview: "Use frontend workspace".to_string(),
            }],
        };

        assert!(workspace_matches(&workspace, "frontend"));
        assert!(workspace_matches(&workspace, "uv"));
        assert!(workspace_matches(&workspace, "demo-alias"));
        assert!(workspace_matches(&workspace, "research"));
        assert!(workspace_matches(&workspace, "AGENTS.md"));
        assert!(workspace_matches(&workspace, "frontend workspace"));
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
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].to_string(), "  first");
        assert_eq!(lines[1].to_string(), "  ");
        assert_eq!(lines[2].to_string(), "  second");
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
            agent_context: Vec::new(),
        }
    }
}
