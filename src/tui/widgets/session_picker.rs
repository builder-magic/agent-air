//! Session picker widget for viewing and switching between sessions
//!
//! Full-screen overlay that displays available sessions on the left
//! and session details on the right.

use chrono::{DateTime, Local};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::themes::Theme;

/// Information about a session for display purposes
#[derive(Clone)]
pub struct SessionInfo {
    pub id: i64,
    pub model: String,
    pub context_used: i64,
    pub context_limit: i32,
    pub created_at: DateTime<Local>,
}

impl SessionInfo {
    pub fn new(id: i64, model: String, context_limit: i32) -> Self {
        Self {
            id,
            model,
            context_used: 0,
            context_limit,
            created_at: Local::now(),
        }
    }
}

/// State for the session picker
pub struct SessionPickerState {
    /// Whether the picker is currently visible
    pub active: bool,
    /// Index of the currently selected session
    pub selected_index: usize,
    /// List of sessions to display
    sessions: Vec<SessionInfo>,
    /// Current active session ID (for marking with *)
    current_session_id: i64,
}

impl SessionPickerState {
    pub fn new() -> Self {
        Self {
            active: false,
            selected_index: 0,
            sessions: Vec::new(),
            current_session_id: 0,
        }
    }

    /// Activate the picker with the given sessions
    pub fn activate(&mut self, sessions: Vec<SessionInfo>, current_session_id: i64) {
        self.active = true;
        self.sessions = sessions;
        self.current_session_id = current_session_id;

        // Find and select current session in list
        self.selected_index = self
            .sessions
            .iter()
            .position(|s| s.id == current_session_id)
            .unwrap_or(0);
    }

    /// Deactivate without switching (cancel)
    pub fn cancel(&mut self) {
        self.active = false;
    }

    /// Deactivate after switching (confirm)
    pub fn confirm(&mut self) {
        self.active = false;
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = self.sessions.len() - 1;
        } else {
            self.selected_index -= 1;
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.sessions.len();
    }

    /// Get the currently selected session ID
    pub fn selected_session_id(&self) -> Option<i64> {
        self.sessions.get(self.selected_index).map(|s| s.id)
    }

    /// Get the currently selected session info
    pub fn selected_session(&self) -> Option<&SessionInfo> {
        self.sessions.get(self.selected_index)
    }
}

impl Default for SessionPickerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Render the session picker
pub fn render_session_picker(state: &SessionPickerState, frame: &mut Frame, area: Rect, theme: &Theme) {
    if !state.active {
        return;
    }

    // Clear the area first
    frame.render_widget(Clear, area);

    // Split into main area and bottom help bar
    let main_chunks =
        Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).split(area);

    // Session list (single pane with all info per line)
    render_session_list(state, frame, main_chunks[0], theme);

    // Bottom help bar
    render_help_bar(frame, main_chunks[1], theme);
}

/// Render the session list with all info on each line
fn render_session_list(
    state: &SessionPickerState,
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
) {
    let mut lines = Vec::new();

    // Header
    lines.push(Line::from(""));

    if state.sessions.is_empty() {
        lines.push(Line::from(Span::styled(
            "   No sessions available",
            theme.text(),
        )));
    } else {
        // Calculate max model name length for alignment
        let max_model_len = state
            .sessions
            .iter()
            .map(|s| s.model.len())
            .max()
            .unwrap_or(0);

        for (idx, session) in state.sessions.iter().enumerate() {
            let is_selected = idx == state.selected_index;
            let is_current = session.id == state.current_session_id;

            let marker = if is_current { "*" } else { " " };
            let prefix = if is_selected { " > " } else { "   " };
            let context_str = format_context(session.context_used, session.context_limit);
            let time_str = session.created_at.format("%H:%M:%S").to_string();

            // Format with labels and aligned model name
            let text = format!(
                "{}{} Session {} | Model: {:<width$} | Context: {} | Created: {}",
                prefix,
                marker,
                session.id,
                session.model,
                context_str,
                time_str,
                width = max_model_len
            );

            let style = if is_selected {
                theme.popup_selected_bg().patch(theme.popup_item_selected())
            } else {
                theme.popup_item()
            };

            // Pad to full width for selection highlight
            let inner_width = area.width.saturating_sub(2) as usize;
            let padded = format!("{:<width$}", text, width = inner_width);
            lines.push(Line::from(Span::styled(padded, style)));
        }
    }

    let block = Block::default()
        .title(" Sessions ")
        .borders(Borders::ALL)
        .border_style(theme.popup_border());

    let list = Paragraph::new(lines)
        .block(block)
        .style(theme.background().patch(theme.text()));

    frame.render_widget(list, area);
}

/// Render the help bar at the bottom
fn render_help_bar(frame: &mut Frame, area: Rect, theme: &Theme) {
    let help_text =
        " Arrow keys to navigate | Enter to switch | Esc to cancel | * = current session";
    let help = Paragraph::new(help_text).style(theme.status_help());
    frame.render_widget(help, area);
}

/// Format context usage for display (e.g., "45.2K / 200K")
fn format_context(used: i64, limit: i32) -> String {
    let used_str = format_tokens(used);
    let limit_str = format_tokens(limit as i64);
    format!("{} / {}", used_str, limit_str)
}

/// Format token counts for display (e.g., "4.3K", "200K", "850")
fn format_tokens(tokens: i64) -> String {
    if tokens >= 100_000 {
        format!("{}K", tokens / 1000)
    } else if tokens >= 1000 {
        format!("{:.1}K", tokens as f64 / 1000.0)
    } else {
        format!("{}", tokens)
    }
}
