//! Batch permission panel widget for parallel tool permissions
//!
//! A Ratatui panel that displays multiple permission requests at once,
//! using a simplified UI similar to the single PermissionPanel.
//!
//! # Navigation
//! - Up/Down: Move between options
//! - Enter/Space: Select option
//! - Esc: Cancel (deny all)

use crate::controller::TurnId;
use crate::permissions::{
    BatchPermissionRequest, BatchPermissionResponse, Grant, GrantTarget, PermissionLevel,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::collections::HashSet;

use crate::themes::Theme;

/// Default configuration values for BatchPermissionPanel
pub mod defaults {
    /// Maximum percentage of screen height the panel can use
    pub const MAX_PANEL_PERCENT: u16 = 70;
    /// Selection indicator for focused items
    pub const SELECTION_INDICATOR: &str = " \u{203A} ";
    /// Blank space for non-focused items (same width as indicator)
    pub const NO_INDICATOR: &str = "   ";
    /// Panel title (matches single permission panel)
    pub const TITLE: &str = " Permission Request ";
    /// Help text
    pub const HELP_TEXT: &str =
        " Up/Down: Navigate \u{00B7} Enter/Space: Select \u{00B7} Esc: Cancel";
}

/// Configuration for BatchPermissionPanel widget
#[derive(Clone)]
pub struct BatchPermissionPanelConfig {
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
}

impl Default for BatchPermissionPanelConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl BatchPermissionPanelConfig {
    /// Create a new BatchPermissionPanelConfig with default values
    pub fn new() -> Self {
        Self {
            max_panel_percent: defaults::MAX_PANEL_PERCENT,
            selection_indicator: defaults::SELECTION_INDICATOR.to_string(),
            no_indicator: defaults::NO_INDICATOR.to_string(),
            title: defaults::TITLE.to_string(),
            help_text: defaults::HELP_TEXT.to_string(),
        }
    }
}

/// Options available for batch permission decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchPermissionOption {
    /// Grant permission for these requests only (no persistent grant)
    GrantOnce,
    /// Grant permission for the remainder of the session (these specific resources)
    GrantSession,
    /// Grant permission for ALL operations of this type for the session
    /// (only shown for homogeneous read/write batches)
    GrantAllSimilar,
    /// Deny all permission requests
    Deny,
}

impl BatchPermissionOption {
    /// Get the display label for this option
    pub fn label(&self) -> &'static str {
        match self {
            BatchPermissionOption::GrantOnce => "Grant Once",
            BatchPermissionOption::GrantSession => "Grant for Session",
            BatchPermissionOption::GrantAllSimilar => "Allow All Similar",
            BatchPermissionOption::Deny => "Deny",
        }
    }

    /// Get the description for this option based on batch context
    pub fn description(&self, count: usize, batch_type: Option<PermissionLevel>) -> String {
        match self {
            BatchPermissionOption::GrantOnce => {
                format!("Allow only these {} requests", count)
            }
            BatchPermissionOption::GrantSession => match batch_type {
                Some(PermissionLevel::Read) => format!("Allow reading these {} files", count),
                Some(PermissionLevel::Write) => format!("Allow writing these {} files", count),
                _ => "Allow these for the session".to_string(),
            },
            BatchPermissionOption::GrantAllSimilar => match batch_type {
                Some(PermissionLevel::Read) => "Allow reading any file".to_string(),
                Some(PermissionLevel::Write) => "Allow writing any file".to_string(),
                _ => "Allow all similar actions".to_string(),
            },
            BatchPermissionOption::Deny => "Deny all requests".to_string(),
        }
    }

    /// Returns true if this is a positive (grant) option
    pub fn is_positive(&self) -> bool {
        !matches!(self, BatchPermissionOption::Deny)
    }
}

/// Result of handling a key event
#[derive(Debug, Clone)]
pub enum BatchKeyAction {
    /// No action taken
    None,
    /// User submitted a batch response
    Submitted(BatchPermissionResponse),
    /// User cancelled (pressed Escape)
    Cancelled(String),
}

/// State for the batch permission panel
pub struct BatchPermissionPanel {
    /// Whether the panel is active/visible
    active: bool,
    /// Batch ID for this permission request
    batch_id: String,
    /// Session ID
    session_id: i64,
    /// The batch permission request to display
    batch: BatchPermissionRequest,
    /// Turn ID for context
    turn_id: Option<TurnId>,
    /// Currently selected option index
    selected_idx: usize,
    /// Configuration for display customization
    config: BatchPermissionPanelConfig,
}

impl BatchPermissionPanel {
    /// Create a new inactive batch permission panel
    pub fn new() -> Self {
        Self::with_config(BatchPermissionPanelConfig::new())
    }

    /// Create a new inactive batch permission panel with custom configuration
    pub fn with_config(config: BatchPermissionPanelConfig) -> Self {
        Self {
            active: false,
            batch_id: String::new(),
            session_id: 0,
            batch: BatchPermissionRequest::new("", Vec::new()),
            turn_id: None,
            selected_idx: 0,
            config,
        }
    }

    /// Activate the panel with a batch permission request
    pub fn activate(
        &mut self,
        session_id: i64,
        batch: BatchPermissionRequest,
        turn_id: Option<TurnId>,
    ) {
        self.active = true;
        self.batch_id = batch.batch_id.clone();
        self.session_id = session_id;
        self.batch = batch;
        self.turn_id = turn_id;
        self.selected_idx = 0; // Default to first option (Grant Once)
    }

    /// Deactivate the panel
    pub fn deactivate(&mut self) {
        self.active = false;
        self.batch_id.clear();
        self.batch = BatchPermissionRequest::new("", Vec::new());
        self.turn_id = None;
        self.selected_idx = 0;
    }

    /// Check if the panel is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get the current batch ID
    pub fn batch_id(&self) -> &str {
        &self.batch_id
    }

    /// Get the session ID
    pub fn session_id(&self) -> i64 {
        self.session_id
    }

    /// Get the current batch request
    pub fn batch(&self) -> &BatchPermissionRequest {
        &self.batch
    }

    /// Get the turn ID
    pub fn turn_id(&self) -> Option<&TurnId> {
        self.turn_id.as_ref()
    }

    /// Determines if this is a homogeneous file batch (all reads or all writes).
    ///
    /// Returns `Some(PermissionLevel::Read)` if all requests are file reads,
    /// `Some(PermissionLevel::Write)` if all requests are file writes,
    /// or `None` for commands or mixed types.
    fn homogeneous_file_level(&self) -> Option<PermissionLevel> {
        if self.batch.requests.is_empty() {
            return None;
        }

        let mut all_reads = true;
        let mut all_writes = true;

        for request in &self.batch.requests {
            // Must be a path target (not domain or command)
            match &request.target {
                GrantTarget::Path { .. } => {
                    if request.required_level != PermissionLevel::Read {
                        all_reads = false;
                    }
                    if request.required_level != PermissionLevel::Write {
                        all_writes = false;
                    }
                }
                GrantTarget::Domain { .. }
                | GrantTarget::Command { .. }
                | GrantTarget::Tool { .. } => {
                    // Any non-path target means not homogeneous file batch
                    return None;
                }
            }
        }

        if all_reads {
            Some(PermissionLevel::Read)
        } else if all_writes {
            Some(PermissionLevel::Write)
        } else {
            None
        }
    }

    /// Get available options based on batch content
    fn available_options(&self) -> Vec<BatchPermissionOption> {
        match self.homogeneous_file_level() {
            Some(PermissionLevel::Read) | Some(PermissionLevel::Write) => {
                // Homogeneous file batch: show all 4 options
                vec![
                    BatchPermissionOption::GrantOnce,
                    BatchPermissionOption::GrantSession,
                    BatchPermissionOption::GrantAllSimilar,
                    BatchPermissionOption::Deny,
                ]
            }
            _ => {
                // Commands or mixed: only 3 options (no "Allow All" for commands)
                vec![
                    BatchPermissionOption::GrantOnce,
                    BatchPermissionOption::GrantSession,
                    BatchPermissionOption::Deny,
                ]
            }
        }
    }

    /// Get the currently selected option
    pub fn selected_option(&self) -> BatchPermissionOption {
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

    /// Build a response based on the selected option
    fn build_response(&self, option: BatchPermissionOption) -> BatchPermissionResponse {
        match option {
            BatchPermissionOption::GrantOnce => {
                // No persistent grants, just approve requests
                BatchPermissionResponse {
                    batch_id: self.batch_id.clone(),
                    approved_grants: self
                        .batch
                        .requests
                        .iter()
                        .map(|r| Grant::new(r.target.clone(), r.required_level))
                        .collect(),
                    denied_requests: HashSet::new(),
                    auto_approved: HashSet::new(),
                }
            }
            BatchPermissionOption::GrantSession => {
                // Use suggested grants from batch
                BatchPermissionResponse::all_granted(
                    &self.batch_id,
                    self.batch.suggested_grants.clone(),
                )
            }
            BatchPermissionOption::GrantAllSimilar => {
                // Create broad grant for all paths with same level
                let level = self
                    .homogeneous_file_level()
                    .unwrap_or(PermissionLevel::Read);
                let broad_grant = Grant::new(GrantTarget::path("/", true), level);
                BatchPermissionResponse::all_granted(&self.batch_id, vec![broad_grant])
            }
            BatchPermissionOption::Deny => {
                let request_ids: Vec<String> =
                    self.batch.requests.iter().map(|r| r.id.clone()).collect();
                BatchPermissionResponse::all_denied(&self.batch_id, request_ids)
            }
        }
    }

    /// Handle a key event
    pub fn process_key(&mut self, key: KeyEvent) -> BatchKeyAction {
        if !self.active {
            return BatchKeyAction::None;
        }

        match key.code {
            // Navigation
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_prev();
                BatchKeyAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                BatchKeyAction::None
            }

            // Selection
            KeyCode::Enter | KeyCode::Char(' ') => {
                let option = self.selected_option();
                let response = self.build_response(option);
                BatchKeyAction::Submitted(response)
            }

            // Cancel
            KeyCode::Esc => {
                let batch_id = self.batch_id.clone();
                BatchKeyAction::Cancelled(batch_id)
            }

            _ => BatchKeyAction::None,
        }
    }

    /// Calculate the height needed for the panel
    pub fn panel_height(&self, max_height: u16) -> u16 {
        // Help text: 1
        // Blank after help: 1
        // "Permissions requested:" label: 1
        // Blank after label: 1
        // Permissions list: 1 per request
        // Separator (blank): 1
        // Options: 3 or 4
        // Bottom padding: 1
        // Borders: 2

        let mut lines = 6u16; // Help (1) + blank (1) + label (1) + blank (1) + separator (1) + bottom padding (1)
        lines += self.batch.requests.len().min(10) as u16; // Cap at 10 to prevent overflow
        lines += self.available_options().len() as u16;
        lines += 2; // Borders

        let max_from_percent = (max_height * self.config.max_panel_percent) / 100;
        lines
            .min(max_from_percent)
            .min(max_height.saturating_sub(6))
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
        let batch_type = self.homogeneous_file_level();

        // Help text at top
        lines.push(Line::from(Span::styled(
            truncate_text(&self.config.help_text, inner_width),
            theme.help_text(),
        )));
        lines.push(Line::from("")); // Blank line

        // "Permissions requested:" label
        lines.push(Line::from(Span::styled(
            " Permissions requested:",
            theme.muted_text().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from("")); // Blank line

        // Permissions list (read-only, no focus indicators or checkboxes)
        let max_requests = 10; // Limit displayed requests to prevent overflow
        for (idx, request) in self.batch.requests.iter().take(max_requests).enumerate() {
            let (target_icon, target_desc) = format_target(&request.target);
            let level_str = format_level(request.required_level);

            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(format!("{} ", target_icon), theme.category()),
                Span::styled(format!("{}: ", level_str), theme.muted_text()),
                Span::styled(
                    truncate_text(&target_desc, inner_width.saturating_sub(20)),
                    theme.resource(),
                ),
            ]));

            // Show "and N more..." if there are too many
            if idx == max_requests - 1 && self.batch.requests.len() > max_requests {
                let remaining = self.batch.requests.len() - max_requests;
                lines.push(Line::from(vec![
                    Span::styled("   ", Style::default()),
                    Span::styled(format!("   ... and {} more", remaining), theme.muted_text()),
                ]));
            }
        }

        // Separator line
        lines.push(Line::from(""));

        // Options
        let options = self.available_options();
        let request_count = self.batch.requests.len();

        for (idx, option) in options.iter().enumerate() {
            let is_selected = idx == self.selected_idx;
            let prefix = if is_selected {
                &self.config.selection_indicator
            } else {
                &self.config.no_indicator
            };

            let description = option.description(request_count, batch_type);

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

impl Default for BatchPermissionPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a grant target for display
fn format_target(target: &GrantTarget) -> (&'static str, String) {
    match target {
        GrantTarget::Path { path, recursive } => {
            let rec_suffix = if *recursive { " (recursive)" } else { "" };
            ("\u{1F4C4}", format!("{}{}", path.display(), rec_suffix))
        }
        GrantTarget::Domain { pattern } => ("\u{2194}", pattern.clone()),
        GrantTarget::Command { pattern } => ("\u{2295}", pattern.clone()),
        GrantTarget::Tool { tool_name } => ("\u{1F527}", tool_name.clone()),
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

use super::{Widget, WidgetAction, WidgetKeyContext, WidgetKeyResult, widget_ids};
use std::any::Any;

impl Widget for BatchPermissionPanel {
    fn id(&self) -> &'static str {
        widget_ids::BATCH_PERMISSION_PANEL
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
            let response = self.build_response(option);
            return WidgetKeyResult::Action(WidgetAction::SubmitBatchPermission {
                batch_id: self.batch_id.clone(),
                response,
            });
        }

        // Cancel using nav helper
        if ctx.nav.is_cancel(&key) {
            let batch_id = self.batch_id.clone();
            return WidgetKeyResult::Action(WidgetAction::CancelBatchPermission { batch_id });
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
    use crate::permissions::PermissionRequest;

    fn create_read_batch() -> BatchPermissionRequest {
        let requests = vec![
            PermissionRequest::file_read("1", "/project/src/main.rs"),
            PermissionRequest::file_read("2", "/project/src/lib.rs"),
            PermissionRequest::file_read("3", "/project/src/utils.rs"),
        ];
        BatchPermissionRequest::new("batch-1", requests)
    }

    fn create_write_batch() -> BatchPermissionRequest {
        let requests = vec![
            PermissionRequest::file_write("1", "/project/src/main.rs"),
            PermissionRequest::file_write("2", "/project/src/lib.rs"),
        ];
        BatchPermissionRequest::new("batch-2", requests)
    }

    fn create_mixed_batch() -> BatchPermissionRequest {
        let requests = vec![
            PermissionRequest::file_read("1", "/project/src/main.rs"),
            PermissionRequest::file_write("2", "/project/src/lib.rs"),
        ];
        BatchPermissionRequest::new("batch-3", requests)
    }

    fn create_command_batch() -> BatchPermissionRequest {
        let requests = vec![
            PermissionRequest::command_execute("1", "git status"),
            PermissionRequest::command_execute("2", "git diff"),
        ];
        BatchPermissionRequest::new("batch-4", requests)
    }

    #[test]
    fn test_panel_activation() {
        let mut panel = BatchPermissionPanel::new();
        assert!(!panel.is_active());

        let batch = create_read_batch();
        panel.activate(1, batch, None);

        assert!(panel.is_active());
        assert_eq!(panel.batch_id(), "batch-1");
        assert_eq!(panel.session_id(), 1);
        assert_eq!(panel.selected_idx, 0);

        panel.deactivate();
        assert!(!panel.is_active());
    }

    #[test]
    fn test_homogeneous_read_batch() {
        let mut panel = BatchPermissionPanel::new();
        let batch = create_read_batch();
        panel.activate(1, batch, None);

        assert_eq!(panel.homogeneous_file_level(), Some(PermissionLevel::Read));
        assert_eq!(panel.available_options().len(), 4); // Includes GrantAllSimilar
    }

    #[test]
    fn test_homogeneous_write_batch() {
        let mut panel = BatchPermissionPanel::new();
        let batch = create_write_batch();
        panel.activate(1, batch, None);

        assert_eq!(panel.homogeneous_file_level(), Some(PermissionLevel::Write));
        assert_eq!(panel.available_options().len(), 4); // Includes GrantAllSimilar
    }

    #[test]
    fn test_mixed_batch_no_grant_all() {
        let mut panel = BatchPermissionPanel::new();
        let batch = create_mixed_batch();
        panel.activate(1, batch, None);

        assert_eq!(panel.homogeneous_file_level(), None);
        assert_eq!(panel.available_options().len(), 3); // No GrantAllSimilar
    }

    #[test]
    fn test_command_batch_no_grant_all() {
        let mut panel = BatchPermissionPanel::new();
        let batch = create_command_batch();
        panel.activate(1, batch, None);

        assert_eq!(panel.homogeneous_file_level(), None);
        assert_eq!(panel.available_options().len(), 3); // No GrantAllSimilar for commands
    }

    #[test]
    fn test_navigation() {
        let mut panel = BatchPermissionPanel::new();
        let batch = create_read_batch();
        panel.activate(1, batch, None);

        // Default is first option
        assert_eq!(panel.selected_option(), BatchPermissionOption::GrantOnce);

        // Move down
        panel.select_next();
        assert_eq!(panel.selected_option(), BatchPermissionOption::GrantSession);

        panel.select_next();
        assert_eq!(
            panel.selected_option(),
            BatchPermissionOption::GrantAllSimilar
        );

        panel.select_next();
        assert_eq!(panel.selected_option(), BatchPermissionOption::Deny);

        // Wrap around
        panel.select_next();
        assert_eq!(panel.selected_option(), BatchPermissionOption::GrantOnce);

        // Move up (wrap)
        panel.select_prev();
        assert_eq!(panel.selected_option(), BatchPermissionOption::Deny);
    }

    #[test]
    fn test_grant_once_response() {
        let mut panel = BatchPermissionPanel::new();
        let batch = create_read_batch();
        panel.activate(1, batch, None);

        let response = panel.build_response(BatchPermissionOption::GrantOnce);
        assert_eq!(response.batch_id, "batch-1");
        assert_eq!(response.approved_grants.len(), 3); // One grant per request
        assert!(response.denied_requests.is_empty());
    }

    #[test]
    fn test_grant_session_response() {
        let mut panel = BatchPermissionPanel::new();
        let batch = create_read_batch();
        panel.activate(1, batch, None);

        let response = panel.build_response(BatchPermissionOption::GrantSession);
        assert_eq!(response.batch_id, "batch-1");
        assert!(!response.approved_grants.is_empty()); // Uses suggested grants
        assert!(response.denied_requests.is_empty());
    }

    #[test]
    fn test_grant_all_similar_response() {
        let mut panel = BatchPermissionPanel::new();
        let batch = create_read_batch();
        panel.activate(1, batch, None);

        let response = panel.build_response(BatchPermissionOption::GrantAllSimilar);
        assert_eq!(response.batch_id, "batch-1");
        assert_eq!(response.approved_grants.len(), 1); // Single broad grant
        assert!(response.denied_requests.is_empty());

        // Check that it's a broad grant
        let grant = &response.approved_grants[0];
        assert_eq!(grant.level, PermissionLevel::Read);
        if let GrantTarget::Path { path, recursive } = &grant.target {
            assert_eq!(path.to_str().unwrap(), "/");
            assert!(*recursive);
        } else {
            panic!("Expected path target");
        }
    }

    #[test]
    fn test_deny_response() {
        let mut panel = BatchPermissionPanel::new();
        let batch = create_read_batch();
        panel.activate(1, batch, None);

        let response = panel.build_response(BatchPermissionOption::Deny);
        assert_eq!(response.batch_id, "batch-1");
        assert!(response.approved_grants.is_empty());
        assert_eq!(response.denied_requests.len(), 3);
    }

    #[test]
    fn test_option_descriptions() {
        let option = BatchPermissionOption::GrantOnce;
        assert_eq!(
            option.description(3, Some(PermissionLevel::Read)),
            "Allow only these 3 requests"
        );

        let option = BatchPermissionOption::GrantSession;
        assert_eq!(
            option.description(3, Some(PermissionLevel::Read)),
            "Allow reading these 3 files"
        );
        assert_eq!(
            option.description(2, Some(PermissionLevel::Write)),
            "Allow writing these 2 files"
        );
        assert_eq!(option.description(5, None), "Allow these for the session");

        let option = BatchPermissionOption::GrantAllSimilar;
        assert_eq!(
            option.description(3, Some(PermissionLevel::Read)),
            "Allow reading any file"
        );
        assert_eq!(
            option.description(2, Some(PermissionLevel::Write)),
            "Allow writing any file"
        );

        let option = BatchPermissionOption::Deny;
        assert_eq!(option.description(3, None), "Deny all requests");
    }
}
