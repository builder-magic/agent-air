// Theme system for the TUI
//
// Centralizes all colors, modifiers, and styles for consistent theming.
// Supports runtime theme switching via RwLock.

use std::sync::RwLock;

use crate::tui::Theme as ThemeTrait;
use ratatui::style::{Color, Modifier, Style};

/// Global theme instance with runtime switching support
static THEME: RwLock<Option<Theme>> = RwLock::new(None);

/// Current theme name
static THEME_NAME: RwLock<Option<String>> = RwLock::new(None);

/// Initialize the global theme (call once at startup)
pub fn init_theme(name: &str, theme: Theme) {
    if let Ok(mut guard) = THEME.write() {
        *guard = Some(theme);
    }
    if let Ok(mut guard) = THEME_NAME.write() {
        *guard = Some(name.to_string());
    }
}

/// Set a new theme at runtime
pub fn set_theme(name: &str, theme: Theme) {
    if let Ok(mut guard) = THEME.write() {
        *guard = Some(theme);
    }
    if let Ok(mut guard) = THEME_NAME.write() {
        *guard = Some(name.to_string());
    }
}

/// Get the current theme name
pub fn current_theme_name() -> String {
    THEME_NAME
        .read()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_else(|| "default".to_string())
}

/// Get the current theme (cloned for safe access)
pub fn theme() -> Theme {
    THEME
        .read()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_default()
}

/// Complete theme definition for the TUI
#[derive(Clone)]
pub struct Theme {
    // Base
    pub background: Style,
    pub text: Style,

    // Borders & Chrome
    pub border: Style,
    pub border_focused: Style,

    // Chat Title Bar
    pub title_separator: Style,
    pub title_indicator_connected: Style,
    pub title_indicator_disconnected: Style,
    pub title_text: Style,

    // Message Roles
    pub user_prefix: Style,
    pub system_prefix: Style,
    pub assistant_prefix: Style,
    pub timestamp: Style,

    // Streaming
    pub cursor: Style,

    // Markdown - Inline
    pub bold: Modifier,
    pub italic: Modifier,
    pub strikethrough: Modifier,
    pub inline_code: Style,
    pub link_text: Style,
    pub link_url: Style,

    // Markdown - Headings
    pub heading_1: Style,
    pub heading_2: Style,
    pub heading_3: Style,
    pub heading_4: Style,

    // Markdown - Code Blocks
    pub code_block: Style,

    // Tables
    pub table_header: Style,
    pub table_cell: Style,
    pub table_border: Style,

    // Tool Execution
    pub tool_header: Style,
    pub tool_executing: Style,
    pub tool_completed: Style,
    pub tool_failed: Style,

    // Input Area
    pub input_border: Style,
    pub input_text: Style,
    pub prompt: Style,

    // Throbber/Progress
    pub throbber_label: Style,
    pub throbber_spinner: Style,

    // Status Bar
    pub status_help: Style,
    pub status_model: Style,

    // Slash Command Popup
    pub popup_border: Style,
    pub popup_header: Style,
    pub popup_item: Style,
    pub popup_item_selected: Style,
    pub popup_item_desc: Style,
    pub popup_item_desc_selected: Style,
    pub popup_selected_bg: Style,
    pub popup_empty: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            // Base
            background: Style::default(),
            text: Style::default(),

            // Borders & Chrome
            border: Style::default().fg(Color::DarkGray),
            border_focused: Style::default().fg(Color::Cyan),

            // Chat Title Bar
            title_separator: Style::default().fg(Color::DarkGray),
            title_indicator_connected: Style::default().fg(Color::Green),
            title_indicator_disconnected: Style::default().fg(Color::Red),
            title_text: Style::default(),

            // Message Roles
            user_prefix: Style::default().fg(Color::Blue),
            system_prefix: Style::default().fg(Color::Yellow),
            assistant_prefix: Style::default().fg(Color::Magenta),
            timestamp: Style::default().fg(Color::DarkGray),

            // Streaming
            cursor: Style::default().fg(Color::Green),

            // Markdown - Inline (modifiers only, preserve existing fg color)
            bold: Modifier::BOLD,
            italic: Modifier::ITALIC,
            strikethrough: Modifier::CROSSED_OUT,
            inline_code: Style::default().fg(Color::Yellow),
            link_text: Style::default().fg(Color::Cyan),
            link_url: Style::default().fg(Color::DarkGray),

            // Markdown - Headings
            heading_1: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            heading_2: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            heading_3: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            heading_4: Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),

            // Markdown - Code Blocks
            code_block: Style::default().fg(Color::Rgb(180, 180, 180)),

            // Tables
            table_header: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            table_cell: Style::default().fg(Color::White),
            table_border: Style::default().fg(Color::DarkGray),

            // Tool Execution
            tool_header: Style::default().fg(Color::Cyan),
            tool_executing: Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC),
            tool_completed: Style::default().fg(Color::Green),
            tool_failed: Style::default().fg(Color::Red),

            // Input Area
            input_border: Style::default().fg(Color::DarkGray),
            input_text: Style::default(),
            prompt: Style::default(),

            // Throbber/Progress
            throbber_label: Style::default().fg(Color::DarkGray),
            throbber_spinner: Style::default().fg(Color::Cyan),

            // Status Bar
            status_help: Style::default().fg(Color::DarkGray),
            status_model: Style::default().fg(Color::DarkGray),

            // Slash Command Popup
            popup_border: Style::default().fg(Color::DarkGray),
            popup_header: Style::default().fg(Color::DarkGray),
            popup_item: Style::default().fg(Color::White),
            popup_item_selected: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            popup_item_desc: Style::default().fg(Color::DarkGray),
            popup_item_desc_selected: Style::default().fg(Color::Gray),
            popup_selected_bg: Style::default().bg(Color::DarkGray),
            popup_empty: Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        }
    }
}

// Implement llm_tui_rs::Theme trait to allow using this theme with shared components
impl ThemeTrait for Theme {
    fn border(&self) -> Style { self.border }
    fn border_focused(&self) -> Style { self.border_focused }
    fn bold(&self) -> Modifier { self.bold }
    fn italic(&self) -> Modifier { self.italic }
    fn strikethrough(&self) -> Modifier { self.strikethrough }
    fn inline_code(&self) -> Style { self.inline_code }
    fn link_text(&self) -> Style { self.link_text }
    fn link_url(&self) -> Style { self.link_url }
    fn heading_1(&self) -> Style { self.heading_1 }
    fn heading_2(&self) -> Style { self.heading_2 }
    fn heading_3(&self) -> Style { self.heading_3 }
    fn heading_4(&self) -> Style { self.heading_4 }
    fn code_block(&self) -> Style { self.code_block }
    fn assistant_prefix(&self) -> Style { self.assistant_prefix }
    fn table_header(&self) -> Style { self.table_header }
    fn table_cell(&self) -> Style { self.table_cell }
    fn table_border(&self) -> Style { self.table_border }
    fn popup_border(&self) -> Style { self.popup_border }
    fn popup_header(&self) -> Style { self.popup_header }
    fn popup_item(&self) -> Style { self.popup_item }
    fn popup_item_selected(&self) -> Style { self.popup_item_selected }
    fn popup_item_desc(&self) -> Style { self.popup_item_desc }
    fn popup_item_desc_selected(&self) -> Style { self.popup_item_desc_selected }
    fn popup_selected_bg(&self) -> Style { self.popup_selected_bg }
    fn popup_empty(&self) -> Style { self.popup_empty }
    fn status_help(&self) -> Style { self.status_help }
    fn background(&self) -> Style { self.background }
    fn text(&self) -> Style { self.text }
    fn cursor(&self) -> Style { self.cursor }
}
