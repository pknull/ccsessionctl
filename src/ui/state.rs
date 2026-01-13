use std::collections::HashSet;

use crate::session::Session;

/// Application view modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    List,
    Preview,
    Search,
    Help,
    Confirm,
}

/// Dialog action to perform on confirmation
#[derive(Debug, Clone)]
pub enum DialogAction {
    DeleteSelected,
    DeleteOlderThan(u32),
    ArchiveSelected,
    ExportSelected,
}

/// Sort field options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortField {
    #[default]
    Date,
    Size,
    Project,
    Name,
}

impl SortField {
    pub fn next(self) -> Self {
        match self {
            SortField::Date => SortField::Size,
            SortField::Size => SortField::Project,
            SortField::Project => SortField::Name,
            SortField::Name => SortField::Date,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SortField::Date => "Date",
            SortField::Size => "Size",
            SortField::Project => "Project",
            SortField::Name => "Name",
        }
    }
}

/// Filter state
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub query: String,
    pub project: Option<String>,
    pub age_days: Option<u32>,
}

/// Main UI state
pub struct UiState {
    pub view: View,
    pub sessions: Vec<Session>,
    pub filtered_indices: Vec<usize>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub selected: HashSet<usize>,
    pub select_mode: bool,
    pub filter: Filter,
    pub preview_scroll: usize,
    pub preview_lines: Vec<String>,
    pub preview_search: String,
    pub preview_search_active: bool,
    pub preview_matches: Vec<usize>,
    pub preview_match_index: usize,
    pub dialog_message: Option<String>,
    pub dialog_action: Option<DialogAction>,
    pub status_message: Option<String>,
    pub projects: Vec<String>,
    pub project_filter_index: usize,
    pub sort_field: SortField,
    pub sort_reversed: bool,
}

impl UiState {
    pub fn new(sessions: Vec<Session>) -> Self {
        let projects = crate::session::get_project_names(&sessions);
        let filtered_indices: Vec<usize> = (0..sessions.len()).collect();

        Self {
            view: View::List,
            sessions,
            filtered_indices,
            cursor: 0,
            scroll_offset: 0,
            selected: HashSet::new(),
            select_mode: false,
            filter: Filter::default(),
            preview_scroll: 0,
            preview_lines: Vec::new(),
            preview_search: String::new(),
            preview_search_active: false,
            preview_matches: Vec::new(),
            preview_match_index: 0,
            dialog_message: None,
            dialog_action: None,
            status_message: None,
            projects,
            project_filter_index: 0, // 0 = All
            sort_field: SortField::Date,
            sort_reversed: false,
        }
    }

    /// Get the currently highlighted session
    pub fn current_session(&self) -> Option<&Session> {
        self.filtered_indices
            .get(self.cursor)
            .and_then(|&idx| self.sessions.get(idx))
    }

    /// Get current session index in the full sessions list
    pub fn current_session_index(&self) -> Option<usize> {
        self.filtered_indices.get(self.cursor).copied()
    }

    /// Move cursor up
    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.adjust_scroll();
        }
    }

    /// Move cursor down
    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.filtered_indices.len() {
            self.cursor += 1;
            self.adjust_scroll();
        }
    }

    /// Move cursor to top
    pub fn cursor_top(&mut self) {
        self.cursor = 0;
        self.scroll_offset = 0;
    }

    /// Move cursor to bottom
    pub fn cursor_bottom(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.cursor = self.filtered_indices.len() - 1;
            self.adjust_scroll();
        }
    }

    /// Page up
    pub fn page_up(&mut self, page_size: usize) {
        self.cursor = self.cursor.saturating_sub(page_size);
        self.adjust_scroll();
    }

    /// Page down
    pub fn page_down(&mut self, page_size: usize) {
        let max = self.filtered_indices.len().saturating_sub(1);
        self.cursor = (self.cursor + page_size).min(max);
        self.adjust_scroll();
    }

    /// Adjust scroll to keep cursor visible
    fn adjust_scroll(&mut self) {
        // Assume visible height of ~20 rows for now
        // This will be adjusted by the UI based on actual terminal size
        let visible_height = 20;

        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        } else if self.cursor >= self.scroll_offset + visible_height {
            self.scroll_offset = self.cursor - visible_height + 1;
        }
    }

    /// Set visible height and readjust scroll
    pub fn set_visible_height(&mut self, height: usize) {
        if self.cursor >= self.scroll_offset + height {
            self.scroll_offset = self.cursor.saturating_sub(height - 1);
        }
    }

    /// Toggle selection on current item
    pub fn toggle_selection(&mut self) {
        if let Some(idx) = self.current_session_index() {
            if self.selected.contains(&idx) {
                self.selected.remove(&idx);
            } else {
                self.selected.insert(idx);
            }
        }
    }

    /// Select all filtered items
    pub fn select_all(&mut self) {
        for &idx in &self.filtered_indices {
            self.selected.insert(idx);
        }
    }

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    /// Check if an index is selected
    pub fn is_selected(&self, idx: usize) -> bool {
        self.selected.contains(&idx)
    }

    /// Get selected sessions
    pub fn get_selected_sessions(&self) -> Vec<&Session> {
        self.selected
            .iter()
            .filter_map(|&idx| self.sessions.get(idx))
            .collect()
    }

    /// Apply filters and update filtered_indices
    pub fn apply_filters(&mut self) {
        use chrono::Utc;

        let now = Utc::now();
        let query_lower = self.filter.query.to_lowercase();

        self.filtered_indices = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| {
                // Project filter
                if let Some(ref proj) = self.filter.project {
                    if &session.project != proj {
                        return false;
                    }
                }

                // Age filter
                if let Some(days) = self.filter.age_days {
                    let age = now.signed_duration_since(session.modified);
                    if age.num_days() < days as i64 {
                        return false;
                    }
                }

                // Query filter (case-insensitive substring match on full content)
                if !query_lower.is_empty() {
                    // Search full content if available, otherwise fall back to metadata
                    let matches = if let Some(ref content) = session.search_content {
                        content.contains(&query_lower)
                    } else {
                        let search_text = format!(
                            "{} {} {} {}",
                            session.project,
                            session.id,
                            session.summary.as_deref().unwrap_or(""),
                            session.first_message.as_deref().unwrap_or("")
                        ).to_lowercase();
                        search_text.contains(&query_lower)
                    };
                    if !matches {
                        return false;
                    }
                }

                true
            })
            .map(|(idx, _)| idx)
            .collect();

        // Reset cursor if out of bounds
        if self.cursor >= self.filtered_indices.len() {
            self.cursor = self.filtered_indices.len().saturating_sub(1);
        }
        self.scroll_offset = 0;
    }

    /// Cycle project filter
    pub fn cycle_project_filter(&mut self) {
        self.project_filter_index = (self.project_filter_index + 1) % (self.projects.len() + 1);

        if self.project_filter_index == 0 {
            self.filter.project = None;
        } else {
            self.filter.project = Some(self.projects[self.project_filter_index - 1].clone());
        }

        self.apply_filters();
    }

    /// Get current project filter display name
    pub fn current_project_filter(&self) -> &str {
        if self.project_filter_index == 0 {
            "All"
        } else {
            &self.projects[self.project_filter_index - 1]
        }
    }

    /// Cycle to next sort field
    pub fn cycle_sort_field(&mut self) {
        self.sort_field = self.sort_field.next();
        self.apply_sort();
        self.set_status(format!("Sort: {} {}", self.sort_field.as_str(), if self.sort_reversed { "↑" } else { "↓" }));
    }

    /// Toggle sort direction
    pub fn toggle_sort_direction(&mut self) {
        self.sort_reversed = !self.sort_reversed;
        self.apply_sort();
        self.set_status(format!("Sort: {} {}", self.sort_field.as_str(), if self.sort_reversed { "↑" } else { "↓" }));
    }

    /// Apply current sort to filtered indices
    pub fn apply_sort(&mut self) {
        let sessions = &self.sessions;
        let sort_field = self.sort_field;
        let reversed = self.sort_reversed;

        self.filtered_indices.sort_by(|&a, &b| {
            let cmp = match sort_field {
                SortField::Date => sessions[b].modified.cmp(&sessions[a].modified),
                SortField::Size => sessions[b].size_bytes.cmp(&sessions[a].size_bytes),
                SortField::Project => sessions[a].project.cmp(&sessions[b].project),
                SortField::Name => {
                    let name_a = sessions[a].summary.as_deref()
                        .or(sessions[a].first_message.as_deref())
                        .unwrap_or("");
                    let name_b = sessions[b].summary.as_deref()
                        .or(sessions[b].first_message.as_deref())
                        .unwrap_or("");
                    name_a.cmp(name_b)
                }
            };
            if reversed { cmp.reverse() } else { cmp }
        });

        // Reset cursor if out of bounds
        if self.cursor >= self.filtered_indices.len() {
            self.cursor = self.filtered_indices.len().saturating_sub(1);
        }
    }

    /// Show confirmation dialog
    pub fn show_confirm(&mut self, message: String, action: DialogAction) {
        self.dialog_message = Some(message);
        self.dialog_action = Some(action);
        self.view = View::Confirm;
    }

    /// Clear dialog
    pub fn clear_dialog(&mut self) {
        self.dialog_message = None;
        self.dialog_action = None;
        self.view = View::List;
    }

    /// Set status message
    pub fn set_status(&mut self, message: String) {
        self.status_message = Some(message);
    }

    /// Clear status message
    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    /// Remove sessions by indices (after deletion)
    pub fn remove_sessions(&mut self, indices: &HashSet<usize>) {
        // Remove from sessions (in reverse order to maintain indices)
        let mut indices_vec: Vec<usize> = indices.iter().copied().collect();
        indices_vec.sort();
        indices_vec.reverse();

        for idx in indices_vec {
            if idx < self.sessions.len() {
                self.sessions.remove(idx);
            }
        }

        // Clear selection
        self.selected.clear();

        // Reapply filters
        self.apply_filters();

        // Update projects list
        self.projects = crate::session::get_project_names(&self.sessions);
    }

    /// Update preview search and find matches
    pub fn update_preview_search(&mut self) {
        self.preview_matches.clear();
        self.preview_match_index = 0;

        if self.preview_search.is_empty() {
            return;
        }

        let query = self.preview_search.to_lowercase();
        for (i, line) in self.preview_lines.iter().enumerate() {
            if line.to_lowercase().contains(&query) {
                self.preview_matches.push(i);
            }
        }

        // Jump to first match
        if !self.preview_matches.is_empty() {
            self.preview_scroll = self.preview_matches[0];
        }
    }

    /// Go to next search match
    pub fn next_preview_match(&mut self) {
        if self.preview_matches.is_empty() {
            return;
        }
        self.preview_match_index = (self.preview_match_index + 1) % self.preview_matches.len();
        self.preview_scroll = self.preview_matches[self.preview_match_index];
    }

    /// Go to previous search match
    pub fn prev_preview_match(&mut self) {
        if self.preview_matches.is_empty() {
            return;
        }
        if self.preview_match_index == 0 {
            self.preview_match_index = self.preview_matches.len() - 1;
        } else {
            self.preview_match_index -= 1;
        }
        self.preview_scroll = self.preview_matches[self.preview_match_index];
    }

    /// Clear preview search
    pub fn clear_preview_search(&mut self) {
        self.preview_search.clear();
        self.preview_search_active = false;
        self.preview_matches.clear();
        self.preview_match_index = 0;
    }
}
