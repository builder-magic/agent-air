// Theme picker widget for selecting themes with live preview
//
// Full-screen overlay that displays available themes on the left
// and a live preview on the right.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::theme::{set_theme, Theme};
use super::themes::{get_theme, THEMES};

/// State for the theme picker
pub struct ThemePickerState {
    /// Whether the picker is currently visible
    pub active: bool,
    /// Index of the currently selected theme
    pub selected_index: usize,
    /// Original theme name (to restore on cancel)
    original_theme_name: String,
    /// Original theme (to restore on cancel)
    original_theme: Option<Theme>,
}

impl ThemePickerState {
    pub fn new() -> Self {
        Self {
            active: false,
            selected_index: 0,
            original_theme_name: String::new(),
            original_theme: None,
        }
    }

    /// Activate the picker and save current theme for potential restore
    pub fn activate(&mut self, current_theme_name: &str, current_theme: Theme) {
        self.active = true;
        self.original_theme_name = current_theme_name.to_string();
        self.original_theme = Some(current_theme);

        // Find and select current theme in list
        self.selected_index = THEMES
            .iter()
            .position(|t| t.name == current_theme_name)
            .unwrap_or(0);

        // Apply selected theme for preview
        self.apply_preview();
    }

    /// Deactivate and restore original theme (cancel)
    pub fn cancel(&mut self) {
        if let Some(theme) = self.original_theme.take() {
            set_theme(&self.original_theme_name, theme);
        }
        self.active = false;
    }

    /// Deactivate and keep current theme (confirm)
    pub fn confirm(&mut self) {
        self.active = false;
        self.original_theme = None;
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if THEMES.is_empty() {
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = THEMES.len() - 1;
        } else {
            self.selected_index -= 1;
        }
        self.apply_preview();
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if THEMES.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % THEMES.len();
        self.apply_preview();
    }

    /// Apply the currently selected theme for preview
    fn apply_preview(&self) {
        if let Some(info) = THEMES.get(self.selected_index) {
            if let Some(theme) = get_theme(info.name) {
                set_theme(info.name, theme);
            }
        }
    }

    /// Get the currently selected theme name
    pub fn selected_theme_name(&self) -> Option<&'static str> {
        THEMES.get(self.selected_index).map(|t| t.name)
    }
}

impl Default for ThemePickerState {
    fn default() -> Self {
        Self::new()
    }
}

// --- Widget trait implementation ---

use std::any::Any;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::tui::widgets::{widget_ids, Widget, WidgetAction, WidgetKeyResult};

/// Result of handling a key event in the theme picker
#[derive(Debug, Clone, PartialEq)]
pub enum ThemeKeyAction {
    /// No action taken
    None,
    /// Navigation (up/down) handled
    Navigated,
    /// Theme confirmed
    Confirmed,
    /// Picker was cancelled
    Cancelled,
}

impl ThemePickerState {
    /// Handle a key event
    pub fn process_key(&mut self, key: KeyEvent) -> ThemeKeyAction {
        if !self.active {
            return ThemeKeyAction::None;
        }

        match key.code {
            KeyCode::Up => {
                self.select_previous();
                ThemeKeyAction::Navigated
            }
            KeyCode::Down => {
                self.select_next();
                ThemeKeyAction::Navigated
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.select_previous();
                ThemeKeyAction::Navigated
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.select_next();
                ThemeKeyAction::Navigated
            }
            KeyCode::Enter => {
                self.confirm();
                ThemeKeyAction::Confirmed
            }
            KeyCode::Esc => {
                self.cancel();
                ThemeKeyAction::Cancelled
            }
            _ => ThemeKeyAction::None,
        }
    }
}

impl Widget for ThemePickerState {
    fn id(&self) -> &'static str {
        widget_ids::THEME_PICKER
    }

    fn priority(&self) -> u8 {
        250 // Very high priority - overlay
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn handle_key(&mut self, key: KeyEvent, _theme: &Theme) -> WidgetKeyResult {
        if !self.active {
            return WidgetKeyResult::NotHandled;
        }

        match self.process_key(key) {
            ThemeKeyAction::Confirmed | ThemeKeyAction::Cancelled => {
                WidgetKeyResult::Action(WidgetAction::Close)
            }
            ThemeKeyAction::Navigated => WidgetKeyResult::Handled,
            ThemeKeyAction::None => WidgetKeyResult::Handled,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        render_theme_picker(self, frame, area);
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

/// Render the theme picker
pub fn render_theme_picker(state: &ThemePickerState, frame: &mut Frame, area: Rect) {
    if !state.active {
        return;
    }

    let theme = super::theme::theme();

    // Clear the area first
    frame.render_widget(Clear, area);

    // Split into main area and bottom help bar
    let main_chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    // Split main area into left (theme list) and right (preview)
    let chunks = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[0]);

    // Left panel: theme list
    render_theme_list(state, frame, chunks[0], &theme);

    // Right panel: preview
    render_preview(frame, chunks[1], &theme);

    // Bottom help bar
    render_help_bar(frame, main_chunks[1], &theme);
}

/// Render the theme list panel
fn render_theme_list(state: &ThemePickerState, frame: &mut Frame, area: Rect, theme: &Theme) {
    let mut lines = Vec::new();

    // Header
    lines.push(Line::from(""));

    for (idx, info) in THEMES.iter().enumerate() {
        let is_selected = idx == state.selected_index;
        let is_current = info.name == state.original_theme_name;

        let marker = if is_current { "* " } else { "  " };
        let prefix = if is_selected { " > " } else { "   " };
        let text = format!("{}{}{}", prefix, marker, info.display_name);

        let style = if is_selected {
            theme.popup_selected_bg.patch(theme.popup_item_selected)
        } else {
            theme.popup_item
        };

        // Pad to full width for selection highlight
        let inner_width = area.width.saturating_sub(2) as usize;
        let padded = format!("{:<width$}", text, width = inner_width);
        lines.push(Line::from(Span::styled(padded, style)));
    }

    let block = Block::default()
        .title(" Select Theme ")
        .borders(Borders::ALL)
        .border_style(theme.popup_border);

    let list = Paragraph::new(lines)
        .block(block)
        .style(theme.background.patch(theme.text));

    frame.render_widget(list, area);
}

/// Render the preview panel
fn render_preview(frame: &mut Frame, area: Rect, theme: &Theme) {
    let mut lines = Vec::new();

    // Preview header
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " # Preview",
        theme.heading_1,
    )));
    lines.push(Line::from(""));

    // User message example
    lines.push(Line::from(Span::styled(" > User message example", theme.user_prefix)));
    lines.push(Line::from(Span::styled("   - 10:30:00 AM", theme.timestamp)));
    lines.push(Line::from(""));

    // Markdown examples
    lines.push(Line::from(vec![
        Span::raw(" This is "),
        Span::styled("bold", theme.text.add_modifier(theme.bold)),
        Span::raw(" and "),
        Span::styled("italic", theme.text.add_modifier(theme.italic)),
        Span::raw(" text."),
    ]));
    lines.push(Line::from(vec![
        Span::raw(" Here is "),
        Span::styled("inline code", theme.inline_code),
        Span::raw(" and a "),
        Span::styled("link", theme.link_text),
        Span::raw("."),
    ]));
    lines.push(Line::from(""));

    // Heading examples
    lines.push(Line::from(Span::styled(" ## Heading 2", theme.heading_2)));
    lines.push(Line::from(Span::styled(" ### Heading 3", theme.heading_3)));
    lines.push(Line::from(""));

    // Code block example
    lines.push(Line::from(Span::styled(" ```rust", theme.code_block)));
    lines.push(Line::from(Span::styled(" fn main() { }", theme.code_block)));
    lines.push(Line::from(Span::styled(" ```", theme.code_block)));
    lines.push(Line::from(""));

    // Table example
    lines.push(Line::from(vec![
        Span::styled(" ", theme.table_border),
        Span::styled("Col1", theme.table_header),
        Span::styled(" | ", theme.table_border),
        Span::styled("Col2", theme.table_header),
    ]));
    lines.push(Line::from(Span::styled(" -----|-----", theme.table_border)));
    lines.push(Line::from(vec![
        Span::styled(" ", theme.table_border),
        Span::styled("A", theme.table_cell),
        Span::styled("    | ", theme.table_border),
        Span::styled("B", theme.table_cell),
    ]));
    lines.push(Line::from(""));

    // Tool status examples
    lines.push(Line::from(Span::styled(" Tool executing...", theme.tool_executing)));
    lines.push(Line::from(Span::styled(" Tool completed", theme.tool_completed)));
    lines.push(Line::from(Span::styled(" Tool failed", theme.tool_failed)));
    lines.push(Line::from(""));

    let block = Block::default()
        .title(" Preview ")
        .borders(Borders::ALL)
        .border_style(theme.popup_border);

    let preview = Paragraph::new(lines)
        .block(block)
        .style(theme.background.patch(theme.text));

    frame.render_widget(preview, area);
}

/// Render the help bar at the bottom
fn render_help_bar(frame: &mut Frame, area: Rect, theme: &Theme) {
    let help_text = " Arrow keys to navigate | Enter to accept | Esc to cancel | * = current theme";
    let help = Paragraph::new(help_text)
        .style(theme.status_help);
    frame.render_widget(help, area);
}
