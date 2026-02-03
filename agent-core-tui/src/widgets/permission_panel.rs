//! Permission panel widget for AskForPermissions tool
//!
//! A reusable Ratatui panel that displays permission requests from the LLM
//! and collects user responses (Grant Once / Grant Session / Deny).
//!
//! # Navigation
//! - Up/Down: Move between options
//! - Enter/Space: Select option
//! - Esc: Cancel (deny)

use crate::controller::{PermissionPanelResponse, TurnId};
use crate::permissions::{Grant, GrantTarget, PermissionLevel, PermissionRequest};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::themes::Theme;

/// Default configuration values for PermissionPanel
pub mod defaults {
    /// Maximum percentage of screen height the panel can use
    pub const MAX_PANEL_PERCENT: u16 = 50;
    /// Selection indicator for focused items
    pub const SELECTION_INDICATOR: &str = " \u{203A} ";
    /// Blank space for non-focused items (same width as indicator)
    pub const NO_INDICATOR: &str = "   ";
    /// Panel title
    pub const TITLE: &str = " Permission Request ";
    /// Help text
    pub const HELP_TEXT: &str =
        " Up/Down: Navigate \u{00B7} Enter/Space: Select \u{00B7} Esc: Cancel";
    /// Target type icons
    pub const ICON_PATH: &str = "\u{1F4C4}"; // Document (for paths)
    pub const ICON_DOMAIN: &str = "\u{2194}"; // Network arrow (for domains)
    pub const ICON_COMMAND: &str = "\u{2295}"; // Command/terminal (for commands)
    pub const ICON_OTHER: &str = "\u{25CB}"; // Default fallback
}

/// Configuration for PermissionPanel widget
#[derive(Clone)]
pub struct PermissionPanelConfig {
    /// Maximum percentage of screen height the panel can use
    pub max_panel_percent: u16,
    /// Selection indicator for focused items
    pub selection_indicator: String,
    /// Blank space for non-focused items
    pub no_indicator: String,
    /// Panel title
    pub title: String,
    /// Help text
    pub help_text: String,
    /// Icon for path targets
    pub icon_path: String,
    /// Icon for domain targets
    pub icon_domain: String,
    /// Icon for command targets
    pub icon_command: String,
    /// Icon for other/unknown targets
    pub icon_other: String,
}

impl Default for PermissionPanelConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionPanelConfig {
    /// Create a new PermissionPanelConfig with default values
    pub fn new() -> Self {
        Self {
            max_panel_percent: defaults::MAX_PANEL_PERCENT,
            selection_indicator: defaults::SELECTION_INDICATOR.to_string(),
            no_indicator: defaults::NO_INDICATOR.to_string(),
            title: defaults::TITLE.to_string(),
            help_text: defaults::HELP_TEXT.to_string(),
            icon_path: defaults::ICON_PATH.to_string(),
            icon_domain: defaults::ICON_DOMAIN.to_string(),
            icon_command: defaults::ICON_COMMAND.to_string(),
            icon_other: defaults::ICON_OTHER.to_string(),
        }
    }

    /// Set the maximum panel height percentage
    pub fn with_max_panel_percent(mut self, percent: u16) -> Self {
        self.max_panel_percent = percent;
        self
    }

    /// Set the selection indicator
    pub fn with_selection_indicator(mut self, indicator: impl Into<String>) -> Self {
        self.selection_indicator = indicator.into();
        self
    }

    /// Set the panel title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set the help text
    pub fn with_help_text(mut self, text: impl Into<String>) -> Self {
        self.help_text = text.into();
        self
    }

    /// Set target type icons
    pub fn with_target_icons(
        mut self,
        path: impl Into<String>,
        domain: impl Into<String>,
        command: impl Into<String>,
        other: impl Into<String>,
    ) -> Self {
        self.icon_path = path.into();
        self.icon_domain = domain.into();
        self.icon_command = command.into();
        self.icon_other = other.into();
        self
    }
}

/// Options available for the user to select
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionOption {
    /// Grant permission for this request only (no persistent grant)
    GrantOnce,
    /// Grant permission for the remainder of the session (this specific resource)
    GrantSession,
    /// Grant permission for ALL operations of this type for the session
    GrantAllSession,
    /// Deny the permission request
    Deny,
}

impl PermissionOption {
    /// Get all options in display order
    pub fn all() -> &'static [PermissionOption] {
        &[
            PermissionOption::GrantOnce,
            PermissionOption::GrantSession,
            PermissionOption::GrantAllSession,
            PermissionOption::Deny,
        ]
    }

    /// Get the display label for this option
    pub fn label(&self) -> &'static str {
        match self {
            PermissionOption::GrantOnce => "Grant Once",
            PermissionOption::GrantSession => "Grant for Session",
            PermissionOption::GrantAllSession => "Allow All Similar",
            PermissionOption::Deny => "Deny",
        }
    }

    /// Get the description for this option based on request context
    pub fn description(&self, request: &PermissionRequest) -> String {
        match self {
            PermissionOption::GrantOnce => "Allow only this request".to_string(),
            PermissionOption::GrantSession => match (&request.target, request.required_level) {
                (GrantTarget::Path { .. }, PermissionLevel::Read) => {
                    "Allow reading this file".to_string()
                }
                (GrantTarget::Path { .. }, PermissionLevel::Write) => {
                    "Allow writing this file".to_string()
                }
                (GrantTarget::Command { .. }, _) => "Allow this command".to_string(),
                (GrantTarget::Domain { .. }, _) => "Allow this domain".to_string(),
                _ => "Allow for the session".to_string(),
            },
            PermissionOption::GrantAllSession => match (&request.target, request.required_level) {
                (GrantTarget::Path { .. }, PermissionLevel::Read) => {
                    "Allow reading any file".to_string()
                }
                (GrantTarget::Path { .. }, PermissionLevel::Write) => {
                    "Allow writing any file".to_string()
                }
                (GrantTarget::Command { .. }, _) => "Allow all commands".to_string(),
                (GrantTarget::Domain { .. }, _) => "Allow all domains".to_string(),
                _ => "Allow all similar actions".to_string(),
            },
            PermissionOption::Deny => "Deny this request".to_string(),
        }
    }

    /// Returns true if this is a positive (grant) option
    pub fn is_positive(&self) -> bool {
        !matches!(self, PermissionOption::Deny)
    }

    /// Convert to a response with optional grant based on the request
    pub fn to_response(&self, request: &PermissionRequest) -> PermissionPanelResponse {
        match self {
            PermissionOption::GrantOnce => PermissionPanelResponse {
                granted: true,
                grant: None, // No persistent grant
                message: None,
            },
            PermissionOption::GrantSession => {
                // Create a grant matching the exact request
                let grant = Grant::new(request.target.clone(), request.required_level);
                PermissionPanelResponse {
                    granted: true,
                    grant: Some(grant),
                    message: None,
                }
            }
            PermissionOption::GrantAllSession => {
                // Create a broader grant based on target type, using the requested level
                let grant = match &request.target {
                    GrantTarget::Path { .. } => {
                        Grant::new(GrantTarget::path("/", true), request.required_level)
                    }
                    GrantTarget::Domain { .. } => Grant::domain("*", request.required_level),
                    GrantTarget::Command { .. } => Grant::command("*", request.required_level),
                };
                PermissionPanelResponse {
                    granted: true,
                    grant: Some(grant),
                    message: None,
                }
            }
            PermissionOption::Deny => PermissionPanelResponse {
                granted: false,
                grant: None,
                message: None,
            },
        }
    }
}

/// Result of handling a key event
#[derive(Debug, Clone)]
pub enum KeyAction {
    /// No action taken
    None,
    /// User selected an option (includes tool_use_id and response)
    Selected(String, PermissionPanelResponse),
    /// User cancelled (pressed Escape)
    Cancelled(String),
}

/// State for the permission panel
pub struct PermissionPanel {
    /// Whether the panel is active/visible
    active: bool,
    /// Tool use ID for this permission request
    tool_use_id: String,
    /// Session ID
    session_id: i64,
    /// The permission request to display
    request: PermissionRequest,
    /// Turn ID for context
    turn_id: Option<TurnId>,
    /// Currently selected option index
    selected_idx: usize,
    /// Configuration for display customization
    config: PermissionPanelConfig,
}

impl PermissionPanel {
    /// Create a new inactive permission panel
    pub fn new() -> Self {
        Self::with_config(PermissionPanelConfig::new())
    }

    /// Create a new inactive permission panel with custom configuration
    pub fn with_config(config: PermissionPanelConfig) -> Self {
        Self {
            active: false,
            tool_use_id: String::new(),
            session_id: 0,
            request: PermissionRequest::new(
                "",
                GrantTarget::path("/", false),
                PermissionLevel::None,
                "",
            ),
            turn_id: None,
            selected_idx: 0,
            config,
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &PermissionPanelConfig {
        &self.config
    }

    /// Set a new configuration
    pub fn set_config(&mut self, config: PermissionPanelConfig) {
        self.config = config;
    }

    /// Activate the panel with a permission request
    pub fn activate(
        &mut self,
        tool_use_id: String,
        session_id: i64,
        request: PermissionRequest,
        turn_id: Option<TurnId>,
    ) {
        self.active = true;
        self.tool_use_id = tool_use_id;
        self.session_id = session_id;
        self.request = request;
        self.turn_id = turn_id;
        self.selected_idx = 0; // Default to first option (Grant Once)
    }

    /// Deactivate the panel
    pub fn deactivate(&mut self) {
        self.active = false;
        self.tool_use_id.clear();
        self.request = PermissionRequest::new(
            "",
            GrantTarget::path("/", false),
            PermissionLevel::None,
            "",
        );
        self.turn_id = None;
        self.selected_idx = 0;
    }

    /// Check if the panel is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get the current tool use ID
    pub fn tool_use_id(&self) -> &str {
        &self.tool_use_id
    }

    /// Get the session ID
    pub fn session_id(&self) -> i64 {
        self.session_id
    }

    /// Get the current request
    pub fn request(&self) -> &PermissionRequest {
        &self.request
    }

    /// Get the turn ID
    pub fn turn_id(&self) -> Option<&TurnId> {
        self.turn_id.as_ref()
    }

    /// Get available options based on request type.
    ///
    /// For commands, "Allow All Similar" is not shown (too risky).
    /// For file reads/writes, all 4 options are shown.
    fn available_options(&self) -> Vec<PermissionOption> {
        match &self.request.target {
            GrantTarget::Command { .. } => {
                // No "Allow All" for commands - too risky
                vec![
                    PermissionOption::GrantOnce,
                    PermissionOption::GrantSession,
                    PermissionOption::Deny,
                ]
            }
            _ => {
                // File reads/writes and domains show all options
                vec![
                    PermissionOption::GrantOnce,
                    PermissionOption::GrantSession,
                    PermissionOption::GrantAllSession,
                    PermissionOption::Deny,
                ]
            }
        }
    }

    /// Get the currently selected option
    pub fn selected_option(&self) -> PermissionOption {
        let options = self.available_options();
        options[self.selected_idx.min(options.len() - 1)]
    }

    /// Move selection to the next option
    pub fn select_next(&mut self) {
        let options = self.available_options();
        self.selected_idx = (self.selected_idx + 1) % options.len();
    }

    /// Move selection to the previous option
    pub fn select_prev(&mut self) {
        let options = self.available_options();
        if self.selected_idx == 0 {
            self.selected_idx = options.len() - 1;
        } else {
            self.selected_idx -= 1;
        }
    }

    /// Handle a key event
    pub fn process_key(&mut self, key: KeyEvent) -> KeyAction {
        if !self.active {
            return KeyAction::None;
        }

        match key.code {
            // Navigation
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_prev();
                KeyAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                KeyAction::None
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.select_prev();
                KeyAction::None
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.select_next();
                KeyAction::None
            }

            // Selection
            KeyCode::Enter | KeyCode::Char(' ') => {
                let option = self.selected_option();
                let response = option.to_response(&self.request);
                let tool_use_id = self.tool_use_id.clone();
                KeyAction::Selected(tool_use_id, response)
            }

            // Cancel
            KeyCode::Esc => {
                let tool_use_id = self.tool_use_id.clone();
                KeyAction::Cancelled(tool_use_id)
            }

            _ => KeyAction::None,
        }
    }

    /// Calculate the height needed for the panel
    pub fn panel_height(&self, max_height: u16) -> u16 {
        // Help text: 1
        // Blank after help: 1
        // Permission line: 1
        // Blank (separator): 1
        // Options: 3 or 4 (depending on request type)
        // Bottom padding: 1
        // Borders: 2

        let mut lines = 6u16; // Help (1) + blank (1) + permission (1) + separator (1) + padding (1) + 1
        lines += self.available_options().len() as u16;
        lines += 2; // Borders

        let max_from_percent = (max_height * self.config.max_panel_percent) / 100;
        lines.min(max_from_percent).min(max_height.saturating_sub(6))
    }

    /// Render the panel
    pub fn render_panel(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.active {
            return;
        }

        // Clear the area first
        frame.render_widget(Clear, area);

        let inner_width = area.width.saturating_sub(4) as usize;
        let mut lines: Vec<Line> = Vec::new();

        // Help text at top
        lines.push(Line::from(Span::styled(
            truncate_text(&self.config.help_text, inner_width),
            theme.help_text(),
        )));
        lines.push(Line::from("")); // Blank line

        // Single permission line: icon + level + target
        let (icon, level_str, target_desc) = match &self.request.target {
            GrantTarget::Path { path, recursive } => {
                let rec_suffix = if *recursive { " (recursive)" } else { "" };
                let level = format_level(self.request.required_level);
                (
                    &self.config.icon_path,
                    level,
                    format!("{}{}", path.display(), rec_suffix),
                )
            }
            GrantTarget::Domain { pattern } => (
                &self.config.icon_domain,
                "Access Domain",
                pattern.clone(),
            ),
            GrantTarget::Command { pattern } => (
                &self.config.icon_command,
                "Execute",
                pattern.clone(),
            ),
        };

        lines.push(Line::from(vec![
            Span::styled("   ", theme.muted_text()),
            Span::styled(format!("{} ", icon), theme.category()),
            Span::styled(format!("{}: ", level_str), theme.muted_text()),
            Span::styled(
                truncate_text(&target_desc, inner_width.saturating_sub(20)),
                theme.resource(),
            ),
        ]));

        // Separator line
        lines.push(Line::from(""));

        // Options
        let options = self.available_options();
        for (idx, option) in options.iter().enumerate() {
            let is_selected = idx == self.selected_idx;
            let prefix = if is_selected {
                &self.config.selection_indicator
            } else {
                &self.config.no_indicator
            };

            let description = option.description(&self.request);

            let (label_style, desc_style) = if is_selected {
                if option.is_positive() {
                    (theme.button_confirm_focused(), theme.focused_text())
                } else {
                    (theme.button_cancel_focused(), theme.focused_text())
                }
            } else if option.is_positive() {
                (theme.button_confirm(), theme.muted_text())
            } else {
                (theme.button_cancel(), theme.muted_text())
            };

            let indicator_style = if is_selected {
                theme.focus_indicator()
            } else {
                theme.muted_text()
            };

            lines.push(Line::from(vec![
                Span::styled(prefix.clone(), indicator_style),
                Span::styled(option.label(), label_style),
                Span::styled(" - ", theme.muted_text()),
                Span::styled(description, desc_style),
            ]));
        }

        // Bottom padding
        lines.push(Line::from(""));

        // Build the block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.warning())
            .title(Span::styled(
                self.config.title.clone(),
                theme.warning().add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }
}

impl Default for PermissionPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a permission level for display
fn format_level(level: PermissionLevel) -> &'static str {
    match level {
        PermissionLevel::None => "None",
        PermissionLevel::Read => "Read File",
        PermissionLevel::Write => "Write File",
        PermissionLevel::Execute => "Execute",
        PermissionLevel::Admin => "Admin",
    }
}

/// Truncate text to fit within a maximum width
fn truncate_text(text: &str, max_width: usize) -> String {
    if text.chars().count() <= max_width {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_width.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

// --- Widget trait implementation ---

use super::{widget_ids, Widget, WidgetAction, WidgetKeyContext, WidgetKeyResult};
use std::any::Any;

impl Widget for PermissionPanel {
    fn id(&self) -> &'static str {
        widget_ids::PERMISSION_PANEL
    }

    fn priority(&self) -> u8 {
        200 // High priority - modal panel
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &WidgetKeyContext) -> WidgetKeyResult {
        if !self.active {
            return WidgetKeyResult::NotHandled;
        }

        // Use NavigationHelper for navigation keys
        if ctx.nav.is_move_up(&key) {
            self.select_prev();
            return WidgetKeyResult::Handled;
        }
        if ctx.nav.is_move_down(&key) {
            self.select_next();
            return WidgetKeyResult::Handled;
        }

        // Selection using nav helper
        if ctx.nav.is_select(&key) {
            let option = self.selected_option();
            let response = option.to_response(&self.request);
            let tool_use_id = self.tool_use_id.clone();
            return WidgetKeyResult::Action(WidgetAction::SubmitPermission {
                tool_use_id,
                response,
            });
        }

        // Cancel using nav helper
        if ctx.nav.is_cancel(&key) {
            let tool_use_id = self.tool_use_id.clone();
            return WidgetKeyResult::Action(WidgetAction::CancelPermission { tool_use_id });
        }

        // j/k vim-style navigation (kept for consistency)
        match key.code {
            KeyCode::Char('k') => {
                self.select_prev();
                WidgetKeyResult::Handled
            }
            KeyCode::Char('j') => {
                self.select_next();
                WidgetKeyResult::Handled
            }
            _ => WidgetKeyResult::NotHandled,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        self.render_panel(frame, area, theme);
    }

    fn required_height(&self, max_height: u16) -> u16 {
        if self.active {
            self.panel_height(max_height)
        } else {
            0
        }
    }

    fn blocks_input(&self) -> bool {
        self.active
    }

    fn is_overlay(&self) -> bool {
        false
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_request() -> PermissionRequest {
        PermissionRequest::file_write("test-1", "/tmp/foo.txt")
    }

    #[test]
    fn test_permission_option_all() {
        let options = PermissionOption::all();
        assert_eq!(options.len(), 4);
        assert_eq!(options[0], PermissionOption::GrantOnce);
        assert_eq!(options[1], PermissionOption::GrantSession);
        assert_eq!(options[2], PermissionOption::GrantAllSession);
        assert_eq!(options[3], PermissionOption::Deny);
    }

    #[test]
    fn test_permission_option_to_response() {
        let request = create_test_request();

        let once = PermissionOption::GrantOnce.to_response(&request);
        assert!(once.granted);
        assert!(once.grant.is_none()); // Grant once doesn't persist

        let session = PermissionOption::GrantSession.to_response(&request);
        assert!(session.granted);
        assert!(session.grant.is_some()); // Session grant persists

        let all_session = PermissionOption::GrantAllSession.to_response(&request);
        assert!(all_session.granted);
        assert!(all_session.grant.is_some()); // All session grant persists

        let deny = PermissionOption::Deny.to_response(&request);
        assert!(!deny.granted);
        assert!(deny.grant.is_none());
    }

    #[test]
    fn test_panel_activation() {
        let mut panel = PermissionPanel::new();
        assert!(!panel.is_active());

        let request = PermissionRequest::file_write("test-1", "/tmp/foo.txt");

        panel.activate("tool_123".to_string(), 1, request, None);
        assert!(panel.is_active());
        assert_eq!(panel.tool_use_id(), "tool_123");
        assert_eq!(panel.session_id(), 1);

        panel.deactivate();
        assert!(!panel.is_active());
    }

    #[test]
    fn test_navigation() {
        let mut panel = PermissionPanel::new();
        let request = create_test_request();
        panel.activate("tool_1".to_string(), 1, request, None);

        // Default is first option
        assert_eq!(panel.selected_option(), PermissionOption::GrantOnce);

        // Move down
        panel.select_next();
        assert_eq!(panel.selected_option(), PermissionOption::GrantSession);

        panel.select_next();
        assert_eq!(panel.selected_option(), PermissionOption::GrantAllSession);

        panel.select_next();
        assert_eq!(panel.selected_option(), PermissionOption::Deny);

        // Wrap around
        panel.select_next();
        assert_eq!(panel.selected_option(), PermissionOption::GrantOnce);

        // Move up (wrap)
        panel.select_prev();
        assert_eq!(panel.selected_option(), PermissionOption::Deny);
    }

    #[test]
    fn test_handle_key_navigation() {
        let mut panel = PermissionPanel::new();
        let request = create_test_request();
        panel.activate("tool_1".to_string(), 1, request, None);

        // Down key
        let action = panel.process_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        matches!(action, KeyAction::None);
        assert_eq!(panel.selected_option(), PermissionOption::GrantSession);

        // Up key
        let action = panel.process_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        matches!(action, KeyAction::None);
        assert_eq!(panel.selected_option(), PermissionOption::GrantOnce);
    }

    #[test]
    fn test_handle_key_selection() {
        let mut panel = PermissionPanel::new();
        let request = create_test_request();
        panel.activate("tool_1".to_string(), 1, request, None);

        // Enter to select
        let action = panel.process_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        match action {
            KeyAction::Selected(tool_use_id, response) => {
                assert_eq!(tool_use_id, "tool_1");
                assert!(response.granted);
                assert!(response.grant.is_none()); // GrantOnce doesn't persist
            }
            _ => panic!("Expected Selected action"),
        }
        panel.deactivate();
        assert!(!panel.is_active());
    }

    #[test]
    fn test_handle_key_cancel() {
        let mut panel = PermissionPanel::new();
        let request = create_test_request();
        panel.activate("tool_1".to_string(), 1, request, None);

        // Escape to cancel
        let action = panel.process_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        match action {
            KeyAction::Cancelled(tool_use_id) => {
                assert_eq!(tool_use_id, "tool_1");
            }
            _ => panic!("Expected Cancelled action"),
        }
        panel.deactivate();
        assert!(!panel.is_active());
    }

    #[test]
    fn test_option_descriptions() {
        let read_request = PermissionRequest::file_read("1", "/tmp/foo.txt");
        let write_request = PermissionRequest::file_write("2", "/tmp/bar.txt");
        let cmd_request = PermissionRequest::command_execute("3", "git status");

        // GrantOnce is always the same
        assert_eq!(
            PermissionOption::GrantOnce.description(&read_request),
            "Allow only this request"
        );

        // GrantSession varies by type
        assert_eq!(
            PermissionOption::GrantSession.description(&read_request),
            "Allow reading this file"
        );
        assert_eq!(
            PermissionOption::GrantSession.description(&write_request),
            "Allow writing this file"
        );
        assert_eq!(
            PermissionOption::GrantSession.description(&cmd_request),
            "Allow this command"
        );

        // GrantAllSession varies by type
        assert_eq!(
            PermissionOption::GrantAllSession.description(&read_request),
            "Allow reading any file"
        );
        assert_eq!(
            PermissionOption::GrantAllSession.description(&write_request),
            "Allow writing any file"
        );
        assert_eq!(
            PermissionOption::GrantAllSession.description(&cmd_request),
            "Allow all commands"
        );

        // Deny is always the same
        assert_eq!(
            PermissionOption::Deny.description(&read_request),
            "Deny this request"
        );
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 10), "short");
        assert_eq!(truncate_text("this is a longer text", 10), "this is...");
        assert_eq!(truncate_text("exact", 5), "exact");
    }

    #[test]
    fn test_command_hides_allow_all() {
        let mut panel = PermissionPanel::new();

        // File read: should have 4 options including GrantAllSession
        let read_request = PermissionRequest::file_read("1", "/tmp/foo.txt");
        panel.activate("tool_1".to_string(), 1, read_request, None);
        let options = panel.available_options();
        assert_eq!(options.len(), 4);
        assert!(options.contains(&PermissionOption::GrantAllSession));
        panel.deactivate();

        // Command: should have 3 options, NO GrantAllSession
        let cmd_request = PermissionRequest::command_execute("2", "git status");
        panel.activate("tool_2".to_string(), 1, cmd_request, None);
        let options = panel.available_options();
        assert_eq!(options.len(), 3);
        assert!(!options.contains(&PermissionOption::GrantAllSession));
    }
}
