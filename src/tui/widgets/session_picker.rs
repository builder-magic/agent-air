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

/// Default configuration values for SessionPicker
pub mod defaults {
    /// Current session marker
    pub const CURRENT_MARKER: &str = "*";
    /// No current marker (space)
    pub const NO_MARKER: &str = " ";
    /// Selection prefix for focused items
    pub const SELECTION_PREFIX: &str = " > ";
    /// No selection prefix
    pub const NO_SELECTION_PREFIX: &str = "   ";
    /// Block title
    pub const TITLE: &str = " Sessions ";
    /// Help text
    pub const HELP_TEXT: &str = " Arrow keys to navigate | Enter to switch | Esc to cancel | * = current session";
    /// No sessions message
    pub const NO_SESSIONS_MESSAGE: &str = "   No sessions available";
}

/// Configuration for SessionPicker widget
#[derive(Clone)]
pub struct SessionPickerConfig {
    /// Marker for current session
    pub current_marker: String,
    /// Marker for non-current sessions
    pub no_marker: String,
    /// Prefix for selected/focused item
    pub selection_prefix: String,
    /// Prefix for non-selected items
    pub no_selection_prefix: String,
    /// Block title
    pub title: String,
    /// Help text shown at bottom
    pub help_text: String,
    /// Message when no sessions available
    pub no_sessions_message: String,
}

impl Default for SessionPickerConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionPickerConfig {
    /// Create a new SessionPickerConfig with default values
    pub fn new() -> Self {
        Self {
            current_marker: defaults::CURRENT_MARKER.to_string(),
            no_marker: defaults::NO_MARKER.to_string(),
            selection_prefix: defaults::SELECTION_PREFIX.to_string(),
            no_selection_prefix: defaults::NO_SELECTION_PREFIX.to_string(),
            title: defaults::TITLE.to_string(),
            help_text: defaults::HELP_TEXT.to_string(),
            no_sessions_message: defaults::NO_SESSIONS_MESSAGE.to_string(),
        }
    }

    /// Set the current session marker
    pub fn with_current_marker(mut self, marker: impl Into<String>) -> Self {
        self.current_marker = marker.into();
        self
    }

    /// Set the selection prefix
    pub fn with_selection_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.selection_prefix = prefix.into();
        self
    }

    /// Set the block title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set the help text
    pub fn with_help_text(mut self, text: impl Into<String>) -> Self {
        self.help_text = text.into();
        self
    }

    /// Set the no sessions message
    pub fn with_no_sessions_message(mut self, message: impl Into<String>) -> Self {
        self.no_sessions_message = message.into();
        self
    }
}

/// Information about a session for display purposes
/// Information about an LLM session for display in the session picker.
#[derive(Clone)]
pub struct SessionInfo {
    /// Session identifier.
    pub id: i64,
    /// Model name.
    pub model: String,
    /// Tokens used in context.
    pub context_used: i64,
    /// Maximum context limit.
    pub context_limit: i32,
    /// Session creation timestamp.
    pub created_at: DateTime<Local>,
}

impl SessionInfo {
    /// Create a new session info.
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
    /// Configuration for display customization
    config: SessionPickerConfig,
}

impl SessionPickerState {
    pub fn new() -> Self {
        Self::with_config(SessionPickerConfig::new())
    }

    /// Create a new session picker with custom configuration
    pub fn with_config(config: SessionPickerConfig) -> Self {
        Self {
            active: false,
            selected_index: 0,
            sessions: Vec::new(),
            current_session_id: 0,
            config,
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &SessionPickerConfig {
        &self.config
    }

    /// Set a new configuration
    pub fn set_config(&mut self, config: SessionPickerConfig) {
        self.config = config;
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

// --- Widget trait implementation ---

use std::any::Any;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use super::{widget_ids, Widget, WidgetAction, WidgetKeyContext, WidgetKeyResult};

/// Result of handling a key event in the session picker
#[derive(Debug, Clone, PartialEq)]
pub enum SessionKeyAction {
    /// No action taken
    None,
    /// Session selected (includes session ID)
    Selected(i64),
    /// Picker was cancelled
    Cancelled,
}

impl SessionPickerState {
    /// Handle a key event
    pub fn process_key(&mut self, key: KeyEvent) -> SessionKeyAction {
        if !self.active {
            return SessionKeyAction::None;
        }

        match key.code {
            KeyCode::Up => {
                self.select_previous();
                SessionKeyAction::None
            }
            KeyCode::Down => {
                self.select_next();
                SessionKeyAction::None
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.select_previous();
                SessionKeyAction::None
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.select_next();
                SessionKeyAction::None
            }
            KeyCode::Enter => {
                if let Some(session_id) = self.selected_session_id() {
                    self.confirm();
                    SessionKeyAction::Selected(session_id)
                } else {
                    SessionKeyAction::None
                }
            }
            KeyCode::Esc => {
                self.cancel();
                SessionKeyAction::Cancelled
            }
            _ => SessionKeyAction::None,
        }
    }
}

impl Widget for SessionPickerState {
    fn id(&self) -> &'static str {
        widget_ids::SESSION_PICKER
    }

    fn priority(&self) -> u8 {
        250 // Very high priority - overlay
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &WidgetKeyContext) -> WidgetKeyResult {
        if !self.active {
            return WidgetKeyResult::NotHandled;
        }

        // Use NavigationHelper for key bindings
        if ctx.nav.is_move_up(&key) {
            self.select_previous();
            return WidgetKeyResult::Handled;
        }
        if ctx.nav.is_move_down(&key) {
            self.select_next();
            return WidgetKeyResult::Handled;
        }
        if ctx.nav.is_select(&key) {
            if let Some(session_id) = self.selected_session_id() {
                self.confirm();
                return WidgetKeyResult::Action(WidgetAction::SwitchSession { session_id });
            }
            return WidgetKeyResult::Handled;
        }
        if ctx.nav.is_cancel(&key) {
            self.cancel();
            return WidgetKeyResult::Action(WidgetAction::Close);
        }

        // Other keys are ignored
        WidgetKeyResult::Handled
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        render_session_picker(self, frame, area, theme);
    }

    fn required_height(&self, _available: u16) -> u16 {
        0 // Overlay widget - doesn't need dedicated height
    }

    fn blocks_input(&self) -> bool {
        self.active
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
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
    render_help_bar(state, frame, main_chunks[1], theme);
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
            state.config.no_sessions_message.clone(),
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

            let marker = if is_current { &state.config.current_marker } else { &state.config.no_marker };
            let prefix = if is_selected { &state.config.selection_prefix } else { &state.config.no_selection_prefix };
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
        .title(state.config.title.clone())
        .borders(Borders::ALL)
        .border_style(theme.popup_border());

    let list = Paragraph::new(lines)
        .block(block)
        .style(theme.background().patch(theme.text()));

    frame.render_widget(list, area);
}

/// Render the help bar at the bottom
fn render_help_bar(state: &SessionPickerState, frame: &mut Frame, area: Rect, theme: &Theme) {
    let help = Paragraph::new(state.config.help_text.clone()).style(theme.status_help());
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
