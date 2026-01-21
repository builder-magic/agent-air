//! Slash command popup widget and state
//!
//! Displays an interactive popup when the user types `/` in the input box.
//! Shows filtered commands with keyboard navigation.
//!
//! This is a generic implementation - applications provide their own command
//! definitions via the SlashCommand trait.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::themes::Theme;

/// Default configuration values for SlashPopup
pub mod defaults {
    /// Header text shown at top of popup
    pub const HEADER_TEXT: &str = " Slash command mode \u{2014} Use arrow keys to select, Enter to execute, Esc to cancel";
    /// Message when no commands match
    pub const NO_MATCHES_MESSAGE: &str = " No matching commands";
    /// Command prefix (the slash)
    pub const COMMAND_PREFIX: &str = " /";
    /// Description indent
    pub const DESCRIPTION_INDENT: &str = " ";
}

/// Configuration for SlashPopup widget
#[derive(Clone)]
pub struct SlashPopupConfig {
    /// Header text shown at top of popup
    pub header_text: String,
    /// Message when no commands match the filter
    pub no_matches_message: String,
    /// Prefix shown before command names
    pub command_prefix: String,
    /// Indent for description lines
    pub description_indent: String,
}

impl Default for SlashPopupConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl SlashPopupConfig {
    /// Create a new SlashPopupConfig with default values
    pub fn new() -> Self {
        Self {
            header_text: defaults::HEADER_TEXT.to_string(),
            no_matches_message: defaults::NO_MATCHES_MESSAGE.to_string(),
            command_prefix: defaults::COMMAND_PREFIX.to_string(),
            description_indent: defaults::DESCRIPTION_INDENT.to_string(),
        }
    }

    /// Set the header text
    pub fn with_header_text(mut self, text: impl Into<String>) -> Self {
        self.header_text = text.into();
        self
    }

    /// Set the no matches message
    pub fn with_no_matches_message(mut self, message: impl Into<String>) -> Self {
        self.no_matches_message = message.into();
        self
    }

    /// Set the command prefix
    pub fn with_command_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.command_prefix = prefix.into();
        self
    }
}

/// Trait for slash commands
///
/// Applications implement this trait for their command types.
pub trait SlashCommand {
    /// The command name (without the leading /)
    fn name(&self) -> &str;

    /// A short description of what the command does
    fn description(&self) -> &str;
}

/// State for the slash command popup
pub struct SlashPopupState {
    /// Whether the popup is currently visible
    pub active: bool,
    /// Index of the currently selected command
    pub selected_index: usize,
    /// Number of filtered commands (for bounds checking)
    filtered_count: usize,
    /// Configuration for display customization
    config: SlashPopupConfig,
}

impl SlashPopupState {
    pub fn new() -> Self {
        Self::with_config(SlashPopupConfig::new())
    }

    /// Create a new slash popup with custom configuration
    pub fn with_config(config: SlashPopupConfig) -> Self {
        Self {
            active: false,
            selected_index: 0,
            filtered_count: 0,
            config,
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &SlashPopupConfig {
        &self.config
    }

    /// Set a new configuration
    pub fn set_config(&mut self, config: SlashPopupConfig) {
        self.config = config;
    }

    /// Activate popup and reset state
    pub fn activate(&mut self) {
        self.active = true;
        self.selected_index = 0;
    }

    /// Deactivate popup
    pub fn deactivate(&mut self) {
        self.active = false;
        self.selected_index = 0;
        self.filtered_count = 0;
    }

    /// Update the filtered command count and clamp selection
    pub fn set_filtered_count(&mut self, count: usize) {
        self.filtered_count = count;
        if count > 0 {
            self.selected_index = self.selected_index.min(count - 1);
        } else {
            self.selected_index = 0;
        }
    }

    /// Move selection up (with wrap)
    pub fn select_previous(&mut self) {
        if self.filtered_count == 0 {
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = self.filtered_count - 1;
        } else {
            self.selected_index -= 1;
        }
    }

    /// Move selection down (with wrap)
    pub fn select_next(&mut self) {
        if self.filtered_count == 0 {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.filtered_count;
    }

    /// Get the currently selected index
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// Calculate the popup height based on filtered commands
    pub fn popup_height(&self, max_height: u16) -> u16 {
        let filtered_count = self.filtered_count.max(1);
        // header(1) + blank(1) + commands * 3 lines each + border(2)
        let content_height = 2 + (filtered_count * 3) + 2;
        (content_height as u16).min(max_height.saturating_sub(10))
    }
}

impl Default for SlashPopupState {
    fn default() -> Self {
        Self::new()
    }
}

// --- Widget trait implementation ---

use std::any::Any;
use crossterm::event::{KeyCode, KeyEvent};
use super::{widget_ids, Widget, WidgetAction, WidgetKeyResult};

/// Result of handling a key event in the slash popup
#[derive(Debug, Clone, PartialEq)]
pub enum SlashKeyAction {
    /// No action taken
    None,
    /// Navigation (up/down) handled
    Navigated,
    /// Command selected at given index
    Selected(usize),
    /// Popup was cancelled
    Cancelled,
    /// Character typed (for filtering) - not consumed, App should handle
    CharTyped(char),
    /// Backspace pressed - not consumed, App should handle
    Backspace,
}

impl SlashPopupState {
    /// Handle a key event
    ///
    /// Returns the action. For CharTyped and Backspace, the App needs to
    /// update the input buffer and filtered commands.
    pub fn process_key(&mut self, key: KeyEvent) -> SlashKeyAction {
        if !self.active {
            return SlashKeyAction::None;
        }

        match key.code {
            KeyCode::Up => {
                self.select_previous();
                SlashKeyAction::Navigated
            }
            KeyCode::Down => {
                self.select_next();
                SlashKeyAction::Navigated
            }
            KeyCode::Enter => {
                let idx = self.selected_index;
                SlashKeyAction::Selected(idx)
            }
            KeyCode::Esc => {
                self.deactivate();
                SlashKeyAction::Cancelled
            }
            KeyCode::Backspace => SlashKeyAction::Backspace,
            KeyCode::Char(c) => SlashKeyAction::CharTyped(c),
            _ => {
                self.deactivate();
                SlashKeyAction::Cancelled
            }
        }
    }
}

impl Widget for SlashPopupState {
    fn id(&self) -> &'static str {
        widget_ids::SLASH_POPUP
    }

    fn priority(&self) -> u8 {
        150 // Medium-high priority
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn handle_key(&mut self, key: KeyEvent, _theme: &Theme) -> WidgetKeyResult {
        if !self.active {
            return WidgetKeyResult::NotHandled;
        }

        match self.process_key(key) {
            SlashKeyAction::Selected(idx) => {
                // Note: App still needs to execute the command
                // We'll return an action that tells App which index was selected
                WidgetKeyResult::Action(WidgetAction::ExecuteCommand {
                    command: format!("__SLASH_INDEX_{}", idx),
                })
            }
            SlashKeyAction::Cancelled => WidgetKeyResult::Action(WidgetAction::Close),
            SlashKeyAction::Navigated => WidgetKeyResult::Handled,
            // For these, we return NotHandled so App can update input buffer
            SlashKeyAction::CharTyped(_) | SlashKeyAction::Backspace | SlashKeyAction::None => {
                WidgetKeyResult::NotHandled
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Note: This is a simplified render that doesn't have commands.
        // App should use render_slash_popup directly with filtered commands.
        // This default render shows an empty state.
        if self.active {
            render_slash_popup(self, &[] as &[SimpleCommand], frame, area, theme);
        }
    }

    fn required_height(&self, max_height: u16) -> u16 {
        if self.active {
            self.popup_height(max_height)
        } else {
            0
        }
    }

    fn blocks_input(&self) -> bool {
        false // Input continues to work while popup is shown
    }

    fn is_overlay(&self) -> bool {
        false // Renders in dedicated area
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

/// Render the slash command popup
///
/// # Arguments
/// * `state` - The popup state
/// * `commands` - The filtered list of commands to display
/// * `frame` - The ratatui frame
/// * `area` - The area to render in
/// * `theme` - The theme to use
pub fn render_slash_popup<C: SlashCommand>(
    state: &SlashPopupState,
    commands: &[C],
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
) {
    if !state.active {
        return;
    }

    // Calculate available width inside borders (area width - 2 for left/right borders)
    let inner_width = area.width.saturating_sub(2) as usize;

    // Build lines: header + commands (all with leading space for padding)
    let mut lines = Vec::new();

    // Header line with leading space
    lines.push(Line::from(vec![Span::styled(
        state.config.header_text.clone(),
        theme.popup_header(),
    )]));
    lines.push(Line::from("")); // Blank line after header

    // Command list
    for (idx, cmd) in commands.iter().enumerate() {
        let is_selected = idx == state.selected_index;

        // Command name line with leading space, padded to full width for selected
        let name_text = format!("{}{}", state.config.command_prefix, cmd.name());
        let name_style = if is_selected {
            theme.popup_selected_bg().patch(theme.popup_item_selected())
        } else {
            theme.popup_item()
        };
        if is_selected {
            // Pad to full width for full-row highlight
            let padded = format!("{:<width$}", name_text, width = inner_width);
            lines.push(Line::from(Span::styled(padded, name_style)));
        } else {
            lines.push(Line::from(Span::styled(name_text, name_style)));
        }

        // Description line with leading space, padded to full width for selected
        let desc_text = format!("{}{}", state.config.description_indent, cmd.description());
        let desc_style = if is_selected {
            theme.popup_selected_bg().patch(theme.popup_item_desc_selected())
        } else {
            theme.popup_item_desc()
        };
        if is_selected {
            // Pad to full width for full-row highlight
            let padded = format!("{:<width$}", desc_text, width = inner_width);
            lines.push(Line::from(Span::styled(padded, desc_style)));
        } else {
            lines.push(Line::from(Span::styled(desc_text, desc_style)));
        }

        // Blank line between commands (except last)
        if idx < commands.len() - 1 {
            lines.push(Line::from(""));
        }
    }

    // If no matches, show empty message
    if commands.is_empty() {
        lines.push(Line::from(Span::styled(
            state.config.no_matches_message.clone(),
            theme.popup_empty(),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.popup_border());

    // Clear the area first (for overlay effect)
    frame.render_widget(Clear, area);

    let popup = Paragraph::new(lines).block(block);
    frame.render_widget(popup, area);
}

/// A simple command implementation for testing
#[derive(Clone)]
pub struct SimpleCommand {
    name: String,
    description: String,
}

impl SimpleCommand {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
        }
    }
}

impl SlashCommand for SimpleCommand {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_popup_state_navigation() {
        let mut state = SlashPopupState::new();
        state.activate();
        state.set_filtered_count(3);

        assert_eq!(state.selected_index, 0);

        state.select_next();
        assert_eq!(state.selected_index, 1);

        state.select_next();
        assert_eq!(state.selected_index, 2);

        // Wrap around
        state.select_next();
        assert_eq!(state.selected_index, 0);

        // Wrap backward
        state.select_previous();
        assert_eq!(state.selected_index, 2);
    }

    #[test]
    fn test_popup_state_empty() {
        let mut state = SlashPopupState::new();
        state.activate();
        state.set_filtered_count(0);

        // Should not crash on empty list
        state.select_next();
        state.select_previous();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_simple_command() {
        let cmd = SimpleCommand::new("help", "Show help message");
        assert_eq!(cmd.name(), "help");
        assert_eq!(cmd.description(), "Show help message");
    }
}
