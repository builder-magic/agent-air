// Theme system for the TUI
//
// Centralizes all colors, modifiers, and styles for consistent theming.
// Supports runtime theme switching via RwLock.

use std::sync::RwLock;

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

/// Complete theme definition for the TUI.
///
/// Contains styles for all UI elements including chat messages, markdown formatting,
/// tool execution, input areas, popups, and interactive panels.
#[derive(Clone)]
pub struct Theme {
    // Base
    /// Background style for the entire UI.
    pub background: Style,
    /// Default text style.
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

    // UI Panel styles (permission panel, question panel)
    pub help_text: Style,
    pub muted_text: Style,
    pub focused_text: Style,
    pub focus_indicator: Style,
    pub selected: Style,
    pub unselected: Style,
    pub button_confirm: Style,
    pub button_confirm_focused: Style,
    pub button_cancel: Style,
    pub button_cancel_focused: Style,
    pub warning: Style,
    pub category: Style,
    pub resource: Style,
}

impl Theme {
    // Accessor methods for compatibility with code that used the old trait

    // Markdown styles
    pub fn bold(&self) -> Modifier {
        self.bold
    }
    pub fn italic(&self) -> Modifier {
        self.italic
    }
    pub fn strikethrough(&self) -> Modifier {
        self.strikethrough
    }
    pub fn inline_code(&self) -> Style {
        self.inline_code
    }
    pub fn link_text(&self) -> Style {
        self.link_text
    }
    pub fn link_url(&self) -> Style {
        self.link_url
    }
    pub fn heading_1(&self) -> Style {
        self.heading_1
    }
    pub fn heading_2(&self) -> Style {
        self.heading_2
    }
    pub fn heading_3(&self) -> Style {
        self.heading_3
    }
    pub fn heading_4(&self) -> Style {
        self.heading_4
    }
    pub fn code_block(&self) -> Style {
        self.code_block
    }
    pub fn assistant_prefix(&self) -> Style {
        self.assistant_prefix
    }

    // Table styles
    pub fn table_header(&self) -> Style {
        self.table_header
    }
    pub fn table_cell(&self) -> Style {
        self.table_cell
    }
    pub fn table_border(&self) -> Style {
        self.table_border
    }

    // Border styles
    pub fn border(&self) -> Style {
        self.border
    }
    pub fn border_focused(&self) -> Style {
        self.border_focused
    }

    // Popup styles
    pub fn popup_border(&self) -> Style {
        self.popup_border
    }
    pub fn popup_header(&self) -> Style {
        self.popup_header
    }
    pub fn popup_item(&self) -> Style {
        self.popup_item
    }
    pub fn popup_item_selected(&self) -> Style {
        self.popup_item_selected
    }
    pub fn popup_item_desc(&self) -> Style {
        self.popup_item_desc
    }
    pub fn popup_item_desc_selected(&self) -> Style {
        self.popup_item_desc_selected
    }
    pub fn popup_selected_bg(&self) -> Style {
        self.popup_selected_bg
    }
    pub fn popup_empty(&self) -> Style {
        self.popup_empty
    }

    // Status styles
    pub fn status_help(&self) -> Style {
        self.status_help
    }
    pub fn background(&self) -> Style {
        self.background
    }
    pub fn text(&self) -> Style {
        self.text
    }
    pub fn cursor(&self) -> Style {
        self.cursor
    }

    // UI Panel styles
    pub fn help_text(&self) -> Style {
        self.help_text
    }
    pub fn muted_text(&self) -> Style {
        self.muted_text
    }
    pub fn focused_text(&self) -> Style {
        self.focused_text
    }
    pub fn focus_indicator(&self) -> Style {
        self.focus_indicator
    }
    pub fn selected(&self) -> Style {
        self.selected
    }
    pub fn unselected(&self) -> Style {
        self.unselected
    }
    pub fn button_confirm(&self) -> Style {
        self.button_confirm
    }
    pub fn button_confirm_focused(&self) -> Style {
        self.button_confirm_focused
    }
    pub fn button_cancel(&self) -> Style {
        self.button_cancel
    }
    pub fn button_cancel_focused(&self) -> Style {
        self.button_cancel_focused
    }
    pub fn warning(&self) -> Style {
        self.warning
    }
    pub fn category(&self) -> Style {
        self.category
    }
    pub fn resource(&self) -> Style {
        self.resource
    }
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

            // UI Panel styles
            help_text: Style::default().fg(Color::DarkGray),
            muted_text: Style::default().fg(Color::Gray),
            focused_text: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            focus_indicator: Style::default().fg(Color::Yellow),
            selected: Style::default().fg(Color::Green),
            unselected: Style::default().fg(Color::Gray),
            button_confirm: Style::default().fg(Color::Rgb(60, 120, 60)),
            button_confirm_focused: Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
            button_cancel: Style::default().fg(Color::Rgb(140, 70, 70)),
            button_cancel_focused: Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
            warning: Style::default().fg(Color::Yellow),
            category: Style::default().fg(Color::Magenta),
            resource: Style::default().fg(Color::Cyan),
        }
    }
}
