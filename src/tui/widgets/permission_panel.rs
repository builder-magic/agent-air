//! Permission panel widget for AskForPermissions tool
//!
//! A reusable Ratatui panel that displays permission requests from the LLM
//! and collects user responses (Grant Once / Grant Session / Deny).
//!
//! # Navigation
//! - Up/Down/Ctrl-P/Ctrl-N: Move between options
//! - Enter/Space: Select option
//! - Esc: Cancel (deny)

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::controller::{
    PermissionCategory, PermissionRequest, PermissionResponse, PermissionScope, TurnId,
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::themes::Theme;

/// Maximum percentage of screen height the panel can use
const MAX_PANEL_PERCENT: u16 = 50;

/// Options available for the user to select
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionOption {
    /// Grant permission for this request only
    GrantOnce,
    /// Grant permission for the remainder of the session
    GrantSession,
    /// Deny the permission request
    Deny,
}

impl PermissionOption {
    /// Get all options in display order
    pub fn all() -> &'static [PermissionOption] {
        &[
            PermissionOption::GrantOnce,
            PermissionOption::GrantSession,
            PermissionOption::Deny,
        ]
    }

    /// Get the display label for this option
    pub fn label(&self) -> &'static str {
        match self {
            PermissionOption::GrantOnce => "Grant Once",
            PermissionOption::GrantSession => "Grant for Session",
            PermissionOption::Deny => "Deny",
        }
    }

    /// Get the description for this option
    pub fn description(&self) -> &'static str {
        match self {
            PermissionOption::GrantOnce => "Allow this action this one time",
            PermissionOption::GrantSession => "Allow this action for the rest of the session",
            PermissionOption::Deny => "Reject this permission request",
        }
    }

    /// Convert to a PermissionResponse
    pub fn to_response(&self) -> PermissionResponse {
        match self {
            PermissionOption::GrantOnce => PermissionResponse {
                granted: true,
                scope: Some(PermissionScope::Once),
                message: None,
            },
            PermissionOption::GrantSession => PermissionResponse {
                granted: true,
                scope: Some(PermissionScope::Session),
                message: None,
            },
            PermissionOption::Deny => PermissionResponse {
                granted: false,
                scope: None,
                message: None,
            },
        }
    }
}

/// Result of handling a key event
#[derive(Debug, Clone, PartialEq)]
pub enum KeyAction {
    /// No action taken
    None,
    /// User selected an option (includes tool_use_id and response)
    Selected(String, PermissionResponse),
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
}

impl PermissionPanel {
    /// Create a new inactive permission panel
    pub fn new() -> Self {
        Self {
            active: false,
            tool_use_id: String::new(),
            session_id: 0,
            request: PermissionRequest {
                action: String::new(),
                reason: None,
                resources: Vec::new(),
                category: PermissionCategory::Other,
            },
            turn_id: None,
            selected_idx: 0,
        }
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
        self.request.action.clear();
        self.request.reason = None;
        self.request.resources.clear();
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

    /// Get the currently selected option
    pub fn selected_option(&self) -> PermissionOption {
        PermissionOption::all()[self.selected_idx]
    }

    /// Move selection to the next option
    pub fn select_next(&mut self) {
        let options = PermissionOption::all();
        self.selected_idx = (self.selected_idx + 1) % options.len();
    }

    /// Move selection to the previous option
    pub fn select_prev(&mut self) {
        let options = PermissionOption::all();
        if self.selected_idx == 0 {
            self.selected_idx = options.len() - 1;
        } else {
            self.selected_idx -= 1;
        }
    }

    /// Handle a key event
    ///
    /// Returns the action that should be taken based on the key press.
    pub fn handle_key(&mut self, key: KeyEvent) -> KeyAction {
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
                let response = option.to_response();
                let tool_use_id = self.tool_use_id.clone();
                // Note: don't deactivate here - let the caller do it after processing
                KeyAction::Selected(tool_use_id, response)
            }

            // Cancel
            KeyCode::Esc => {
                let tool_use_id = self.tool_use_id.clone();
                // Note: don't deactivate here - let the caller do it after processing
                KeyAction::Cancelled(tool_use_id)
            }

            _ => KeyAction::None,
        }
    }

    /// Calculate the height needed for the panel
    pub fn panel_height(&self, max_height: u16) -> u16 {
        // Calculate height needed:
        // - Title: 1 line
        // - Blank: 1 line
        // - Category: 1 line
        // - Action: 1 line (may wrap, but estimate 1)
        // - Reason (if present): 1 line
        // - Resources header + items: 1 + resources.len()
        // - Blank: 1 line
        // - Options: 3 lines (one per option)
        // - Blank: 1 line
        // - Help: 1 line
        // - Borders: 2 lines

        let mut lines = 0u16;

        // Header
        lines += 2; // Title + blank

        // Content
        lines += 1; // Category
        lines += 1; // Action
        if self.request.reason.is_some() {
            lines += 1;
        }
        if !self.request.resources.is_empty() {
            lines += 1 + self.request.resources.len().min(5) as u16; // Header + up to 5 resources
        }

        // Options
        lines += 1; // Blank before options
        lines += PermissionOption::all().len() as u16;

        // Help and borders
        lines += 1; // Blank before help
        lines += 1; // Help text
        lines += 2; // Borders

        // Cap at percentage of available height
        let max_from_percent = (max_height * MAX_PANEL_PERCENT) / 100;
        lines.min(max_from_percent).min(max_height.saturating_sub(6))
    }

    /// Render the panel
    ///
    /// # Arguments
    /// * `frame` - The Ratatui frame to render into
    /// * `area` - The area to render the panel in
    /// * `theme` - Theme implementation for styling
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.active {
            return;
        }

        // Clear the area first
        frame.render_widget(Clear, area);

        let inner_width = area.width.saturating_sub(4) as usize;
        let mut lines: Vec<Line> = Vec::new();

        // Help text at top
        let help = " Up/Down: Navigate \u{00B7} Enter/Space: Select \u{00B7} Esc: Cancel";
        lines.push(Line::from(Span::styled(
            truncate_text(help, inner_width),
            theme.help_text(),
        )));
        lines.push(Line::from("")); // Blank line

        // Category with icon
        let category_icon = match self.request.category {
            PermissionCategory::FileWrite => "\u{270E}", // pencil
            PermissionCategory::FileDelete => "\u{2717}", // ballot x
            PermissionCategory::Network => "\u{2194}", // left right arrow
            PermissionCategory::System => "\u{2295}", // circled plus
            PermissionCategory::Other => "\u{25CB}", // white circle
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", category_icon),
                theme.category(),
            ),
            Span::styled(
                format!("{}", self.request.category),
                theme.category().add_modifier(Modifier::BOLD),
            ),
        ]));

        // Action
        lines.push(Line::from(vec![
            Span::styled(" Action: ", theme.muted_text()),
            Span::styled(
                truncate_text(&self.request.action, inner_width - 10),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]));

        // Reason (if present)
        if let Some(ref reason) = self.request.reason {
            lines.push(Line::from(vec![
                Span::styled(" Reason: ", theme.muted_text()),
                Span::styled(
                    truncate_text(reason, inner_width - 10),
                    theme.muted_text(),
                ),
            ]));
        }

        // Resources (if present)
        if !self.request.resources.is_empty() {
            lines.push(Line::from(Span::styled(
                " Resources:",
                theme.muted_text(),
            )));
            for (i, resource) in self.request.resources.iter().take(5).enumerate() {
                let prefix = if i < self.request.resources.len() - 1 || self.request.resources.len() <= 5 {
                    "   \u{251C}\u{2500} " // ├─
                } else {
                    "   \u{2514}\u{2500} " // └─
                };
                lines.push(Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(
                        truncate_text(resource, inner_width - 8),
                        theme.resource(),
                    ),
                ]));
            }
            if self.request.resources.len() > 5 {
                lines.push(Line::from(Span::styled(
                    format!("   ... and {} more", self.request.resources.len() - 5),
                    theme.muted_text(),
                )));
            }
        }

        // Blank line before options
        lines.push(Line::from(""));

        // Selection indicator
        const INDICATOR: &str = " \u{203A} "; // › arrow
        const NO_INDICATOR: &str = "   ";

        // Options
        for (idx, option) in PermissionOption::all().iter().enumerate() {
            let is_selected = idx == self.selected_idx;
            let prefix = if is_selected { INDICATOR } else { NO_INDICATOR };

            let (label_style, desc_style) = if is_selected {
                match option {
                    PermissionOption::GrantOnce | PermissionOption::GrantSession => {
                        (theme.button_confirm_focused(), theme.focused_text())
                    }
                    PermissionOption::Deny => {
                        (theme.button_cancel_focused(), theme.focused_text())
                    }
                }
            } else {
                match option {
                    PermissionOption::GrantOnce | PermissionOption::GrantSession => {
                        (theme.button_confirm(), theme.muted_text())
                    }
                    PermissionOption::Deny => {
                        (theme.button_cancel(), theme.muted_text())
                    }
                }
            };

            let indicator_style = if is_selected {
                theme.focus_indicator()
            } else {
                theme.muted_text()
            };

            lines.push(Line::from(vec![
                Span::styled(prefix, indicator_style),
                Span::styled(option.label(), label_style),
                Span::styled(" - ", theme.muted_text()),
                Span::styled(option.description(), desc_style),
            ]));
        }

        // Build the block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.warning())
            .title(Span::styled(
                " Permission Required ",
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

/// Truncate text to fit within a maximum width
fn truncate_text(text: &str, max_width: usize) -> String {
    if text.chars().count() <= max_width {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_width.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_option_all() {
        let options = PermissionOption::all();
        assert_eq!(options.len(), 3);
        assert_eq!(options[0], PermissionOption::GrantOnce);
        assert_eq!(options[1], PermissionOption::GrantSession);
        assert_eq!(options[2], PermissionOption::Deny);
    }

    #[test]
    fn test_permission_option_to_response() {
        let once = PermissionOption::GrantOnce.to_response();
        assert!(once.granted);
        assert_eq!(once.scope, Some(PermissionScope::Once));

        let session = PermissionOption::GrantSession.to_response();
        assert!(session.granted);
        assert_eq!(session.scope, Some(PermissionScope::Session));

        let deny = PermissionOption::Deny.to_response();
        assert!(!deny.granted);
        assert!(deny.scope.is_none());
    }

    #[test]
    fn test_panel_activation() {
        let mut panel = PermissionPanel::new();
        assert!(!panel.is_active());

        let request = PermissionRequest {
            action: "Delete file".to_string(),
            reason: Some("Cleanup".to_string()),
            resources: vec!["/tmp/foo.txt".to_string()],
            category: PermissionCategory::FileDelete,
        };

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
        let request = PermissionRequest {
            action: "Test".to_string(),
            reason: None,
            resources: vec![],
            category: PermissionCategory::Other,
        };
        panel.activate("tool_1".to_string(), 1, request, None);

        // Default is first option
        assert_eq!(panel.selected_option(), PermissionOption::GrantOnce);

        // Move down
        panel.select_next();
        assert_eq!(panel.selected_option(), PermissionOption::GrantSession);

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
        let request = PermissionRequest {
            action: "Test".to_string(),
            reason: None,
            resources: vec![],
            category: PermissionCategory::Other,
        };
        panel.activate("tool_1".to_string(), 1, request, None);

        // Down key
        let action = panel.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(action, KeyAction::None);
        assert_eq!(panel.selected_option(), PermissionOption::GrantSession);

        // Up key
        let action = panel.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(action, KeyAction::None);
        assert_eq!(panel.selected_option(), PermissionOption::GrantOnce);
    }

    #[test]
    fn test_handle_key_selection() {
        let mut panel = PermissionPanel::new();
        let request = PermissionRequest {
            action: "Test".to_string(),
            reason: None,
            resources: vec![],
            category: PermissionCategory::Other,
        };
        panel.activate("tool_1".to_string(), 1, request, None);

        // Enter to select
        let action = panel.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        match action {
            KeyAction::Selected(tool_use_id, response) => {
                assert_eq!(tool_use_id, "tool_1");
                assert!(response.granted);
                assert_eq!(response.scope, Some(PermissionScope::Once));
            }
            _ => panic!("Expected Selected action"),
        }
        // Panel doesn't deactivate itself - caller must do it
        panel.deactivate();
        assert!(!panel.is_active());
    }

    #[test]
    fn test_handle_key_cancel() {
        let mut panel = PermissionPanel::new();
        let request = PermissionRequest {
            action: "Test".to_string(),
            reason: None,
            resources: vec![],
            category: PermissionCategory::Other,
        };
        panel.activate("tool_1".to_string(), 1, request, None);

        // Escape to cancel
        let action = panel.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        match action {
            KeyAction::Cancelled(tool_use_id) => {
                assert_eq!(tool_use_id, "tool_1");
            }
            _ => panic!("Expected Cancelled action"),
        }
        // Panel doesn't deactivate itself - caller must do it
        panel.deactivate();
        assert!(!panel.is_active());
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 10), "short");
        assert_eq!(truncate_text("this is a longer text", 10), "this is...");
        assert_eq!(truncate_text("exact", 5), "exact");
    }
}
