use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
    Frame,
};
use std::io::{self, Write};
use std::process::{Command, Stdio};

use super::highlight::{parse_code_blocks, CodeBlockInfo, Highlighter};
use super::state::{DialogAction, UiState, View};
use crate::actions;
use crate::session::{get_session_preview, load_session_messages, load_session_metadata};

fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

/// Decode project path from Claude's directory encoding
/// e.g., "-home-pknull-dotfiles" -> "/home/pknull/dotfiles"
fn decode_project_path(raw_name: &str) -> String {
    let path = raw_name.strip_prefix('-').unwrap_or(raw_name);
    format!("/{}", path.replace('-', "/"))
}

fn copy_to_clipboard(text: &str) -> bool {
    // Try xclip first (X11), then xsel, then wl-copy (Wayland)
    let commands = [
        ("xclip", vec!["-selection", "clipboard"]),
        ("xsel", vec!["--clipboard", "--input"]),
        ("wl-copy", vec![]),
    ];

    for (cmd, args) in &commands {
        if let Ok(mut child) = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                if stdin.write_all(text.as_bytes()).is_ok() {
                    return child.wait().map(|s| s.success()).unwrap_or(false);
                }
            }
        }
    }
    false
}

pub struct App {
    pub state: UiState,
    pub should_quit: bool,
    needs_refresh: bool,
    table_state: TableState,
    highlighter: Highlighter,
    code_blocks: Vec<CodeBlockInfo>,
}

impl App {
    pub fn new(state: UiState) -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));

        Self {
            state,
            should_quit: false,
            needs_refresh: false,
            table_state,
            highlighter: Highlighter::new(),
            code_blocks: Vec::new(),
        }
    }

    pub fn run(&mut self, terminal: &mut ratatui::Terminal<impl Backend>) -> Result<()> {
        // Load all metadata upfront for accurate display
        self.load_all_metadata(terminal)?;

        while !self.should_quit {
            terminal.draw(|f| self.draw(f))?;
            self.handle_events()?;

            // Handle refresh with terminal access for progress display
            if self.needs_refresh {
                self.needs_refresh = false;
                self.do_refresh(terminal)?;
            }
        }

        Ok(())
    }

    fn load_all_metadata(&mut self, terminal: &mut ratatui::Terminal<impl Backend>) -> Result<()> {
        let total = self.state.sessions.len();

        for (i, session) in self.state.sessions.iter_mut().enumerate() {
            if session.first_message.is_none() {
                let _ = load_session_metadata(session);
            }

            // Update progress every 50 sessions
            if i % 50 == 0 || i == total - 1 {
                terminal.draw(|f| {
                    let area = f.size();
                    let msg = format!("Loading session metadata... {}/{}", i + 1, total);
                    let paragraph = ratatui::widgets::Paragraph::new(msg)
                        .style(ratatui::style::Style::default().fg(ratatui::style::Color::Cyan))
                        .alignment(ratatui::layout::Alignment::Center);
                    f.render_widget(paragraph, area);
                })?;
            }
        }

        Ok(())
    }

    fn load_all_metadata_sync(&mut self) {
        for session in self.state.sessions.iter_mut() {
            if session.first_message.is_none() {
                let _ = load_session_metadata(session);
            }
        }
    }

    fn handle_events(&mut self) -> Result<()> {
        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        return Ok(());
                    }

                    // Clear status message on any key
                    self.state.clear_status();

                    match self.state.view {
                        View::List => self.handle_list_keys(key.code, key.modifiers),
                        View::Preview => self.handle_preview_keys(key.code),
                        View::Search => self.handle_search_keys(key.code),
                        View::Help => self.handle_help_keys(key.code),
                        View::Confirm => self.handle_confirm_keys(key.code),
                    }
                }
                Event::Mouse(mouse) => {
                    self.handle_mouse(mouse.kind);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_mouse(&mut self, kind: MouseEventKind) {
        match kind {
            MouseEventKind::ScrollUp => {
                match self.state.view {
                    View::List => {
                        self.state.cursor_up();
                        self.table_state.select(Some(self.state.cursor));
                    }
                    View::Preview => {
                        self.state.preview_scroll = self.state.preview_scroll.saturating_sub(3);
                    }
                    _ => {}
                }
            }
            MouseEventKind::ScrollDown => {
                match self.state.view {
                    View::List => {
                        self.state.cursor_down();
                        self.table_state.select(Some(self.state.cursor));
                    }
                    View::Preview => {
                        if self.state.preview_scroll + 3 < self.state.preview_lines.len() {
                            self.state.preview_scroll += 3;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn handle_list_keys(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.state.cursor_down();
                self.table_state.select(Some(self.state.cursor));
                self.load_current_metadata();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.state.cursor_up();
                self.table_state.select(Some(self.state.cursor));
                self.load_current_metadata();
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.state.cursor_top();
                self.table_state.select(Some(0));
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.state.cursor_bottom();
                self.table_state
                    .select(Some(self.state.filtered_indices.len().saturating_sub(1)));
            }
            KeyCode::PageUp => {
                self.state.page_up(20);
                self.table_state.select(Some(self.state.cursor));
            }
            KeyCode::PageDown => {
                self.state.page_down(20);
                self.table_state.select(Some(self.state.cursor));
            }
            KeyCode::Enter => {
                self.open_preview();
            }
            KeyCode::Char(' ') => {
                self.state.toggle_selection();
                self.state.cursor_down();
                self.table_state.select(Some(self.state.cursor));
            }
            KeyCode::Char('v') => {
                self.state.select_mode = !self.state.select_mode;
            }
            KeyCode::Char('a') => {
                self.state.select_all();
            }
            KeyCode::Char('A') => {
                self.state.clear_selection();
            }
            KeyCode::Char('/') => {
                self.state.view = View::Search;
            }
            KeyCode::Char('?') => {
                self.state.view = View::Help;
            }
            KeyCode::Char('p') => {
                self.state.cycle_project_filter();
            }
            KeyCode::Char('d') => {
                self.confirm_delete();
            }
            KeyCode::Char('D') => {
                self.confirm_delete_older();
            }
            KeyCode::Char('e') => {
                self.do_export();
            }
            KeyCode::Char('z') => {
                self.do_archive();
            }
            KeyCode::Char('r') => {
                self.needs_refresh = true;
            }
            KeyCode::Char('s') => {
                self.state.cycle_sort_field();
                self.table_state.select(Some(self.state.cursor));
            }
            KeyCode::Char('o') => {
                self.state.toggle_sort_direction();
                self.table_state.select(Some(self.state.cursor));
            }
            KeyCode::Char('y') => {
                if let Some(session) = self.state.get_current_session() {
                    let project_dir = decode_project_path(&session.project_raw);
                    let cmd = format!("cd {} && claude --resume {}", project_dir, session.id);
                    if copy_to_clipboard(&cmd) {
                        self.state.set_status(format!("Copied: {}", cmd));
                    } else {
                        self.state.set_status("Failed to copy (xclip not found?)".to_string());
                    }
                }
            }
            KeyCode::Char('Y') => {
                if let Some(session) = self.state.get_current_session() {
                    let path = session.path.display().to_string();
                    if copy_to_clipboard(&path) {
                        self.state.set_status(format!("Copied path: {}", path));
                    } else {
                        self.state.set_status("Failed to copy (xclip not found?)".to_string());
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_preview_keys(&mut self, code: KeyCode) {
        // If search is active, handle search input
        if self.state.preview_search_active {
            match code {
                KeyCode::Esc => {
                    self.state.clear_preview_search();
                }
                KeyCode::Enter => {
                    self.state.preview_search_active = false;
                }
                KeyCode::Backspace => {
                    self.state.preview_search.pop();
                    self.state.update_preview_search();
                }
                KeyCode::Char(c) => {
                    self.state.preview_search.push(c);
                    self.state.update_preview_search();
                }
                _ => {}
            }
            return;
        }

        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.state.clear_preview_search();
                self.state.view = View::List;
                self.state.preview_lines.clear();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.state.preview_scroll + 1 < self.state.preview_lines.len() {
                    self.state.preview_scroll += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.state.preview_scroll = self.state.preview_scroll.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.state.preview_scroll = (self.state.preview_scroll + 20)
                    .min(self.state.preview_lines.len().saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.state.preview_scroll = self.state.preview_scroll.saturating_sub(20);
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.state.preview_scroll = 0;
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.state.preview_scroll = self.state.preview_lines.len().saturating_sub(1);
            }
            KeyCode::Char('/') => {
                self.state.preview_search_active = true;
            }
            KeyCode::Char('n') => {
                self.state.next_preview_match();
            }
            KeyCode::Char('N') => {
                self.state.prev_preview_match();
            }
            _ => {}
        }
    }

    fn handle_search_keys(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.state.filter.query.clear();
                self.state.apply_filters();
                self.state.view = View::List;
            }
            KeyCode::Enter => {
                self.state.apply_filters();
                self.state.view = View::List;
            }
            KeyCode::Backspace => {
                self.state.filter.query.pop();
                self.state.apply_filters();
            }
            KeyCode::Char(c) => {
                self.state.filter.query.push(c);
                self.state.apply_filters();
            }
            _ => {}
        }
    }

    fn handle_help_keys(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('?') => {
                self.state.view = View::List;
            }
            _ => {}
        }
    }

    fn handle_confirm_keys(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y') | KeyCode::Enter => {
                if let Some(action) = self.state.dialog_action.take() {
                    self.execute_dialog_action(action);
                }
                self.state.clear_dialog();
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.state.clear_dialog();
            }
            _ => {}
        }
    }

    fn load_current_metadata(&mut self) {
        if let Some(idx) = self.state.current_session_index() {
            if let Some(session) = self.state.sessions.get_mut(idx) {
                if session.first_message.is_none() {
                    let _ = load_session_metadata(session);
                }
            }
        }
    }

    fn open_preview(&mut self) {
        if let Some(session) = self.state.current_session() {
            match load_session_messages(&session.path) {
                Ok(messages) => {
                    self.state.preview_lines = messages
                        .iter()
                        .flat_map(|msg| {
                            let role = match msg.role {
                                crate::session::MessageRole::User => "[User]",
                                crate::session::MessageRole::Assistant => "[Assistant]",
                                crate::session::MessageRole::System => "[System]",
                            };
                            let header = format!(
                                "{} {}",
                                role,
                                msg.timestamp.format("%Y-%m-%d %H:%M:%S")
                            );
                            let mut lines = vec![header, String::new()];
                            lines.extend(msg.content.lines().map(String::from));
                            lines.push(String::new());
                            lines
                        })
                        .collect();
                    // Parse code blocks for syntax highlighting
                    self.code_blocks = parse_code_blocks(&self.state.preview_lines);
                    self.state.preview_scroll = 0;
                    self.state.view = View::Preview;
                }
                Err(e) => {
                    self.state.set_status(format!("Failed to load: {}", e));
                }
            }
        }
    }

    fn confirm_delete(&mut self) {
        let count = if self.state.selected.is_empty() {
            1
        } else {
            self.state.selected.len()
        };

        let msg = if count == 1 {
            "Delete this session? (y/n)".to_string()
        } else {
            format!("Delete {} sessions? (y/n)", count)
        };

        self.state.show_confirm(msg, DialogAction::DeleteSelected);
    }

    fn confirm_delete_older(&mut self) {
        // For simplicity, hardcode 30 days
        let days = 30;
        self.state.show_confirm(
            format!("Delete sessions older than {} days? (y/n)", days),
            DialogAction::DeleteOlderThan(days),
        );
    }

    fn execute_dialog_action(&mut self, action: DialogAction) {
        match action {
            DialogAction::DeleteSelected => {
                let to_delete: std::collections::HashSet<usize> = if self.state.selected.is_empty()
                {
                    self.state
                        .current_session_index()
                        .into_iter()
                        .collect()
                } else {
                    self.state.selected.clone()
                };

                let sessions: Vec<_> = to_delete
                    .iter()
                    .filter_map(|&idx| self.state.sessions.get(idx))
                    .collect();

                let count = sessions.len();
                for session in sessions {
                    let _ = actions::delete_session(session);
                }

                self.state.remove_sessions(&to_delete);
                self.state.set_status(format!("Deleted {} session(s)", count));
            }
            DialogAction::DeleteOlderThan(days) => {
                use chrono::Utc;
                let now = Utc::now();
                let to_delete: std::collections::HashSet<usize> = self
                    .state
                    .sessions
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| {
                        let age = now.signed_duration_since(s.modified);
                        age.num_days() >= days as i64
                    })
                    .map(|(idx, _)| idx)
                    .collect();

                let sessions: Vec<_> = to_delete
                    .iter()
                    .filter_map(|&idx| self.state.sessions.get(idx))
                    .collect();

                let count = sessions.len();
                for session in sessions {
                    let _ = actions::delete_session(session);
                }

                self.state.remove_sessions(&to_delete);
                self.state
                    .set_status(format!("Deleted {} session(s) older than {} days", count, days));
            }
            DialogAction::ArchiveSelected => {
                // Handled in do_archive
            }
            DialogAction::ExportSelected => {
                // Handled in do_export
            }
        }
    }

    fn do_export(&mut self) {
        let sessions: Vec<_> = if self.state.selected.is_empty() {
            self.state.current_session().into_iter().collect()
        } else {
            self.state.get_selected_sessions()
        };

        if sessions.is_empty() {
            self.state.set_status("No sessions to export".to_string());
            return;
        }

        match actions::get_default_export_dir() {
            Ok(dir) => {
                let mut count = 0;
                for session in sessions {
                    if actions::export_session_markdown(session, &dir).is_ok() {
                        count += 1;
                    }
                }
                self.state
                    .set_status(format!("Exported {} session(s) to {:?}", count, dir));
            }
            Err(e) => {
                self.state.set_status(format!("Export failed: {}", e));
            }
        }
    }

    fn do_archive(&mut self) {
        let sessions: Vec<_> = if self.state.selected.is_empty() {
            self.state.current_session().into_iter().collect()
        } else {
            self.state.get_selected_sessions()
        };

        if sessions.is_empty() {
            self.state.set_status("No sessions to archive".to_string());
            return;
        }

        match actions::get_default_archive_dir() {
            Ok(dir) => {
                let mut count = 0;
                for session in sessions {
                    if actions::archive_session(session, &dir).is_ok() {
                        count += 1;
                    }
                }
                self.state
                    .set_status(format!("Archived {} session(s) to {:?}", count, dir));
            }
            Err(e) => {
                self.state.set_status(format!("Archive failed: {}", e));
            }
        }
    }

    fn do_refresh(&mut self, terminal: &mut ratatui::Terminal<impl Backend>) -> Result<()> {
        match crate::session::scan_sessions() {
            Ok(sessions) => {
                let total = sessions.len();
                self.state = UiState::new(sessions);
                self.table_state.select(Some(0));

                // Load all metadata with progress display
                for (i, session) in self.state.sessions.iter_mut().enumerate() {
                    if session.first_message.is_none() {
                        let _ = load_session_metadata(session);
                    }

                    // Update progress display
                    if i % 20 == 0 || i == total - 1 {
                        terminal.draw(|f| {
                            let area = f.size();
                            let msg = format!("Refreshing... {}/{}", i + 1, total);
                            let paragraph = ratatui::widgets::Paragraph::new(msg)
                                .style(ratatui::style::Style::default().fg(ratatui::style::Color::Cyan))
                                .alignment(ratatui::layout::Alignment::Center);
                            f.render_widget(paragraph, area);
                        })?;
                    }
                }

                self.state.set_status(format!("Refreshed: {} sessions", total));
            }
            Err(e) => {
                self.state.set_status(format!("Refresh failed: {}", e));
            }
        }
        Ok(())
    }

    fn draw(&mut self, f: &mut Frame) {
        let size = f.size();

        match self.state.view {
            View::List | View::Search => self.draw_list_view(f, size),
            View::Preview => self.draw_preview_view(f, size),
            View::Help => {
                self.draw_list_view(f, size);
                self.draw_help_overlay(f, size);
            }
            View::Confirm => {
                self.draw_list_view(f, size);
                self.draw_confirm_dialog(f, size);
            }
        }
    }

    fn draw_list_view(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header + filter
                Constraint::Min(5),    // Table
                Constraint::Length(2), // Status + keybinds
            ])
            .split(area);

        // Header
        self.draw_header(f, chunks[0]);

        // Session table
        self.draw_session_table(f, chunks[1]);

        // Footer
        self.draw_footer(f, chunks[2]);
    }

    fn draw_header(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(2)])
            .split(area);

        // Title
        let title = Paragraph::new("ccsessionctl - Claude Code Session Manager")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        f.render_widget(title, chunks[0]);

        // Filter bar
        let filter_text = if self.state.view == View::Search {
            format!("Filter: [{}▏]", self.state.filter.query)
        } else if self.state.filter.query.is_empty() {
            "Filter: [/]".to_string()
        } else {
            format!("Filter: [{}]", self.state.filter.query)
        };

        let project_text = format!("Project: [{}]", self.state.current_project_filter());
        let sort_arrow = if self.state.sort_reversed { "↑" } else { "↓" };
        let sort_text = format!("Sort: [{}{}]", self.state.sort_field.as_str(), sort_arrow);

        let filter_line = Line::from(vec![
            Span::raw(filter_text),
            Span::raw("  "),
            Span::styled(project_text, Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled(sort_text, Style::default().fg(Color::Magenta)),
            Span::raw(format!(
                "  ({}/{})",
                self.state.filtered_indices.len(),
                self.state.sessions.len()
            )),
        ]);

        let filter_bar = Paragraph::new(filter_line)
            .block(Block::default().borders(Borders::BOTTOM));
        f.render_widget(filter_bar, chunks[1]);
    }

    fn draw_session_table(&mut self, f: &mut Frame, area: Rect) {
        let header_cells = ["", "Project", "Date", "Size", "Tokens", "Preview"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().add_modifier(Modifier::BOLD)));
        let header = Row::new(header_cells).height(1);

        let rows: Vec<Row> = self
            .state
            .filtered_indices
            .iter()
            .enumerate()
            .map(|(row_idx, &session_idx)| {
                let session = &self.state.sessions[session_idx];
                let selected = self.state.is_selected(session_idx);

                let sel_marker = if selected { "●" } else { " " };
                let project = &session.project;
                let date = session.modified.format("%b %d").to_string();
                let size = humansize::format_size(session.size_bytes, humansize::BINARY);
                let tokens = session
                    .token_count
                    .map(|t| format_tokens(t))
                    .unwrap_or_else(|| "-".to_string());
                let preview = get_session_preview(session);

                let style = if row_idx == self.state.cursor {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else if selected {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };

                Row::new(vec![
                    Cell::from(sel_marker),
                    Cell::from(project.as_str()),
                    Cell::from(date),
                    Cell::from(size),
                    Cell::from(tokens),
                    Cell::from(preview),
                ])
                .style(style)
            })
            .collect();

        let widths = [
            Constraint::Length(2),
            Constraint::Length(15),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Min(20),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(
                        " Sessions {}",
                        if !self.state.selected.is_empty() {
                            format!("({} selected)", self.state.selected.len())
                        } else {
                            String::new()
                        }
                    )),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        f.render_stateful_widget(table, area, &mut self.table_state);
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(area);

        // Status line
        let status = if let Some(ref msg) = self.state.status_message {
            Span::styled(msg.as_str(), Style::default().fg(Color::Green))
        } else {
            Span::raw("")
        };
        f.render_widget(Paragraph::new(status), chunks[0]);

        // Keybinds
        let keybinds = Line::from(vec![
            Span::styled("j/k", Style::default().fg(Color::Cyan)),
            Span::raw(":Nav "),
            Span::styled("Space", Style::default().fg(Color::Cyan)),
            Span::raw(":Sel "),
            Span::styled("p", Style::default().fg(Color::Cyan)),
            Span::raw(":Project "),
            Span::styled("s", Style::default().fg(Color::Cyan)),
            Span::raw(":Sort "),
            Span::styled("d", Style::default().fg(Color::Cyan)),
            Span::raw(":Del "),
            Span::styled("e", Style::default().fg(Color::Cyan)),
            Span::raw(":Export "),
            Span::styled("r", Style::default().fg(Color::Cyan)),
            Span::raw(":Refresh "),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::raw(":Quit"),
        ]);
        f.render_widget(Paragraph::new(keybinds), chunks[1]);
    }

    fn draw_preview_view(&mut self, f: &mut Frame, area: Rect) {
        let has_search = !self.state.preview_search.is_empty() || self.state.preview_search_active;
        let constraints = if has_search {
            vec![Constraint::Length(1), Constraint::Min(5), Constraint::Length(1)]
        } else {
            vec![Constraint::Min(5), Constraint::Length(1)]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        let (content_area, footer_area) = if has_search {
            // Draw search bar
            let search_text = if self.state.preview_search_active {
                format!("Search: [{}▏]", self.state.preview_search)
            } else {
                let match_info = if !self.state.preview_matches.is_empty() {
                    format!(" ({}/{})", self.state.preview_match_index + 1, self.state.preview_matches.len())
                } else if !self.state.preview_search.is_empty() {
                    " (no matches)".to_string()
                } else {
                    String::new()
                };
                format!("Search: [{}]{}", self.state.preview_search, match_info)
            };
            let search_bar = Paragraph::new(search_text)
                .style(Style::default().fg(Color::Yellow));
            f.render_widget(search_bar, chunks[0]);
            (chunks[1], chunks[2])
        } else {
            (chunks[0], chunks[1])
        };

        // Get session info for title
        let title = if let Some(session) = self.state.current_session() {
            format!(" Preview: {} - {} ", session.project, session.id)
        } else {
            " Preview ".to_string()
        };

        // Pre-compute which lines are in code blocks
        let code_blocks = &self.code_blocks;
        let wrap_width = content_area.width.saturating_sub(2) as usize; // Account for borders

        let items: Vec<ListItem> = self
            .state
            .preview_lines
            .iter()
            .enumerate()
            .skip(self.state.preview_scroll)
            .take(content_area.height as usize)
            .map(|(idx, line)| {
                let is_match = self.state.preview_matches.contains(&idx);

                // Check if this line is in a code block
                let in_code_block = code_blocks.iter().any(|block| {
                    idx >= block.start && idx < block.end
                });
                let is_code_fence = line.starts_with("```");
                let code_block_lang = if is_code_fence && line.len() > 3 {
                    Some(line.trim_start_matches('`').trim())
                } else {
                    None
                };

                // Determine styling
                let (content, base_style) = if line.starts_with("[User]") {
                    (
                        wrap_line(line, wrap_width),
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    )
                } else if line.starts_with("[Assistant]") {
                    (
                        wrap_line(line, wrap_width),
                        Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
                    )
                } else if line.starts_with("[System]") {
                    (
                        wrap_line(line, wrap_width),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    )
                } else if is_code_fence {
                    // Style code fence markers
                    let lang_display = code_block_lang.unwrap_or("");
                    (
                        vec![Line::from(vec![
                            Span::styled("```", Style::default().fg(Color::Magenta)),
                            Span::styled(lang_display.to_string(), Style::default().fg(Color::Cyan).add_modifier(Modifier::ITALIC)),
                        ])],
                        Style::default(),
                    )
                } else if in_code_block {
                    // Don't wrap code blocks to preserve formatting - just truncate or scroll
                    let block = code_blocks.iter().find(|b| idx >= b.start && idx < b.end);
                    if let Some(block) = block {
                        let highlighted = self.highlighter.highlight_code(line, &block.language);
                        if let Some(first_line) = highlighted.into_iter().next() {
                            (vec![first_line], Style::default().bg(Color::Rgb(30, 30, 46)))
                        } else {
                            (vec![Line::from(line.as_str())], Style::default().bg(Color::Rgb(30, 30, 46)))
                        }
                    } else {
                        (vec![Line::from(line.as_str())], Style::default().bg(Color::Rgb(30, 30, 46)))
                    }
                } else if line.starts_with("🔧") {
                    // Tool use - wrap
                    (wrap_line(line, wrap_width), Style::default().fg(Color::Cyan))
                } else if line.starts_with("💭") {
                    // Thinking - wrap
                    (wrap_line(line, wrap_width), Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
                } else if line.starts_with("📋") {
                    // Tool result - wrap
                    (wrap_line(line, wrap_width), Style::default().fg(Color::Gray))
                } else {
                    (wrap_line(line, wrap_width), Style::default())
                };

                // Highlight matched lines
                let final_style = if is_match {
                    base_style.bg(Color::DarkGray)
                } else {
                    base_style
                };

                ListItem::new(content).style(final_style)
            })
            .collect();

        let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));

        f.render_widget(list, content_area);

        // Footer
        let footer = Line::from(vec![
            Span::styled("j/k", Style::default().fg(Color::Cyan)),
            Span::raw(":Scroll "),
            Span::styled("/", Style::default().fg(Color::Cyan)),
            Span::raw(":Search "),
            Span::styled("n/N", Style::default().fg(Color::Cyan)),
            Span::raw(":Next/Prev "),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::raw(":Back"),
        ]);
        f.render_widget(Paragraph::new(footer), footer_area);
    }

    fn draw_help_overlay(&self, f: &mut Frame, area: Rect) {
        let help_text = vec![
            "",
            "  Navigation",
            "  j/k, Up/Down    Move cursor",
            "  g/G, Home/End   Go to top/bottom",
            "  PgUp/PgDn       Page up/down",
            "  Enter           Open preview",
            "",
            "  Selection",
            "  Space           Toggle selection",
            "  v               Visual select mode",
            "  a               Select all",
            "  A               Clear selection",
            "",
            "  Filters & Sort",
            "  /               Search",
            "  p               Cycle project filter",
            "  s               Cycle sort (date/size/project/name)",
            "  o               Toggle sort order",
            "",
            "  Clipboard",
            "  y               Copy resume command",
            "  Y               Copy session path",
            "",
            "  Actions",
            "  d               Delete selected",
            "  D               Delete older than 30 days",
            "  e               Export to Markdown",
            "  z               Archive to tar.gz",
            "  r               Refresh list",
            "",
            "  ?               Toggle help",
            "  q               Quit",
            "",
        ];

        let help_height = help_text.len() as u16 + 2;
        let help_width = 45;

        let popup_area = centered_rect(help_width, help_height, area);

        let help_items: Vec<ListItem> = help_text
            .iter()
            .map(|line| ListItem::new(*line))
            .collect();

        let help = List::new(help_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Help ")
                    .style(Style::default().bg(Color::Black)),
            )
            .style(Style::default().bg(Color::Black));

        f.render_widget(Clear, popup_area);
        f.render_widget(help, popup_area);
    }

    fn draw_confirm_dialog(&self, f: &mut Frame, area: Rect) {
        let msg = self
            .state
            .dialog_message
            .as_deref()
            .unwrap_or("Confirm?");

        let popup_area = centered_rect(50, 5, area);

        let dialog = Paragraph::new(msg)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Confirm ")
                    .style(Style::default().bg(Color::Black)),
            )
            .style(Style::default().bg(Color::Black))
            .wrap(Wrap { trim: true });

        f.render_widget(Clear, popup_area);
        f.render_widget(dialog, popup_area);
    }
}

/// Helper function to create a centered rect
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// Wrap a line of text to fit within the given width
fn wrap_line(text: &str, max_width: usize) -> Vec<Line<'static>> {
    if max_width == 0 {
        return vec![Line::from(text.to_string())];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for word in text.split_inclusive(|c: char| c.is_whitespace()) {
        let word_width = unicode_width::UnicodeWidthStr::width(word);

        if current_width + word_width > max_width && !current_line.is_empty() {
            // Push current line and start new one
            lines.push(Line::from(current_line.clone()));
            current_line.clear();
            current_width = 0;
        }

        // Handle words longer than max_width by breaking them
        if word_width > max_width {
            let mut chars = word.chars().peekable();
            while chars.peek().is_some() {
                let mut chunk = String::new();
                let mut chunk_width = 0;

                while let Some(&c) = chars.peek() {
                    let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                    if current_width + chunk_width + char_width > max_width && !chunk.is_empty() {
                        break;
                    }
                    chunk.push(chars.next().unwrap());
                    chunk_width += char_width;
                }

                if !current_line.is_empty() && current_width + chunk_width > max_width {
                    lines.push(Line::from(current_line.clone()));
                    current_line.clear();
                    current_width = 0;
                }

                current_line.push_str(&chunk);
                current_width += chunk_width;

                if current_width >= max_width {
                    lines.push(Line::from(current_line.clone()));
                    current_line.clear();
                    current_width = 0;
                }
            }
        } else {
            current_line.push_str(word);
            current_width += word_width;
        }
    }

    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }

    if lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines
}
