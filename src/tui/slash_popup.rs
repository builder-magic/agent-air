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

use super::theme::Theme;

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
}

impl SlashPopupState {
    pub fn new() -> Self {
        Self {
            active: false,
            selected_index: 0,
            filtered_count: 0,
        }
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
    theme: &impl Theme,
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
        " Slash command mode \u{2014} Use arrow keys to select, Enter to execute, Esc to cancel",
        theme.popup_header(),
    )]));
    lines.push(Line::from("")); // Blank line after header

    // Command list
    for (idx, cmd) in commands.iter().enumerate() {
        let is_selected = idx == state.selected_index;

        // Command name line with leading space, padded to full width for selected
        let name_text = format!(" /{}", cmd.name());
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
        let desc_text = format!(" {}", cmd.description());
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

    // If no matches, show "No matching commands" with leading space
    if commands.is_empty() {
        lines.push(Line::from(Span::styled(
            " No matching commands",
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
