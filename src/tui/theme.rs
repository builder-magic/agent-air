//! Theme system for LLM TUI components
//!
//! Provides a trait-based theming system that allows consumers to customize
//! the appearance of components while providing sensible defaults.

use ratatui::style::{Color, Modifier, Style};

/// Theme trait for customizing component appearance.
///
/// Implement this trait to provide custom colors and styles for LLM TUI components.
/// A default implementation is provided that uses a standard dark terminal theme.
pub trait Theme {
    /// Style for panel borders
    fn border(&self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// Style for focused/active borders
    fn border_focused(&self) -> Style {
        Style::default().fg(Color::Cyan)
    }

    /// Style for help text (keyboard shortcuts, hints)
    fn help_text(&self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// Style for normal/muted text
    fn muted_text(&self) -> Style {
        Style::default().fg(Color::Gray)
    }

    /// Style for focused/highlighted text
    fn focused_text(&self) -> Style {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    /// Style for the focus indicator (arrow/caret)
    fn focus_indicator(&self) -> Style {
        Style::default().fg(Color::Yellow)
    }

    /// Style for selected items (checkboxes, radio buttons)
    fn selected(&self) -> Style {
        Style::default().fg(Color::Green)
    }

    /// Style for unselected items
    fn unselected(&self) -> Style {
        Style::default().fg(Color::Gray)
    }

    /// Style for the submit/confirm button (unfocused)
    fn button_confirm(&self) -> Style {
        Style::default().fg(Color::Rgb(60, 120, 60))
    }

    /// Style for the submit/confirm button (focused)
    fn button_confirm_focused(&self) -> Style {
        Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)
    }

    /// Style for the cancel/deny button (unfocused)
    fn button_cancel(&self) -> Style {
        Style::default().fg(Color::Rgb(140, 70, 70))
    }

    /// Style for the cancel/deny button (focused)
    fn button_cancel_focused(&self) -> Style {
        Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD)
    }

    /// Style for warning/caution elements
    fn warning(&self) -> Style {
        Style::default().fg(Color::Yellow)
    }

    /// Style for the text cursor
    fn cursor(&self) -> Style {
        Style::default().bg(Color::White).fg(Color::Black)
    }

    /// Style for category labels
    fn category(&self) -> Style {
        Style::default().fg(Color::Magenta)
    }

    /// Style for resource paths/URLs
    fn resource(&self) -> Style {
        Style::default().fg(Color::Cyan)
    }

    // =========================================================================
    // Markdown styles
    // =========================================================================

    /// Modifier for bold text
    fn bold(&self) -> Modifier {
        Modifier::BOLD
    }

    /// Modifier for italic text
    fn italic(&self) -> Modifier {
        Modifier::ITALIC
    }

    /// Modifier for strikethrough text
    fn strikethrough(&self) -> Modifier {
        Modifier::CROSSED_OUT
    }

    /// Style for inline code (`code`)
    fn inline_code(&self) -> Style {
        Style::default().fg(Color::Yellow)
    }

    /// Style for link text
    fn link_text(&self) -> Style {
        Style::default().fg(Color::Cyan)
    }

    /// Style for link URLs
    fn link_url(&self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// Style for heading level 1
    fn heading_1(&self) -> Style {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    /// Style for heading level 2
    fn heading_2(&self) -> Style {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    }

    /// Style for heading level 3
    fn heading_3(&self) -> Style {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    }

    /// Style for heading level 4+
    fn heading_4(&self) -> Style {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    }

    /// Style for code blocks
    fn code_block(&self) -> Style {
        Style::default().fg(Color::Rgb(180, 180, 180))
    }

    /// Style for the assistant message prefix
    fn assistant_prefix(&self) -> Style {
        Style::default().fg(Color::Magenta)
    }

    // =========================================================================
    // Table styles
    // =========================================================================

    /// Style for table header cells
    fn table_header(&self) -> Style {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    /// Style for table data cells
    fn table_cell(&self) -> Style {
        Style::default().fg(Color::White)
    }

    /// Style for table borders
    fn table_border(&self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    // =========================================================================
    // Popup styles
    // =========================================================================

    /// Style for popup borders
    fn popup_border(&self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// Style for popup headers
    fn popup_header(&self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// Style for popup items (unselected)
    fn popup_item(&self) -> Style {
        Style::default().fg(Color::White)
    }

    /// Style for popup items (selected)
    fn popup_item_selected(&self) -> Style {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    /// Style for popup item descriptions (unselected)
    fn popup_item_desc(&self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// Style for popup item descriptions (selected)
    fn popup_item_desc_selected(&self) -> Style {
        Style::default().fg(Color::Gray)
    }

    /// Background style for selected popup items
    fn popup_selected_bg(&self) -> Style {
        Style::default().bg(Color::DarkGray)
    }

    /// Style for empty popup message
    fn popup_empty(&self) -> Style {
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)
    }

    // =========================================================================
    // Status bar styles
    // =========================================================================

    /// Style for status bar help text
    fn status_help(&self) -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// Style for background
    fn background(&self) -> Style {
        Style::default()
    }

    /// Style for normal text
    fn text(&self) -> Style {
        Style::default()
    }
}

/// Default theme implementation using standard terminal colors.
#[derive(Debug, Clone, Default)]
pub struct DefaultTheme;

impl Theme for DefaultTheme {}

/// A theme that wraps style values for runtime configuration.
///
/// Use this when you need to configure theme values at runtime
/// rather than implementing the Theme trait.
#[derive(Debug, Clone)]
pub struct ConfigurableTheme {
    pub border: Style,
    pub border_focused: Style,
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
    pub cursor: Style,
    pub category: Style,
    pub resource: Style,
    // Markdown
    pub bold: Modifier,
    pub italic: Modifier,
    pub strikethrough: Modifier,
    pub inline_code: Style,
    pub link_text: Style,
    pub link_url: Style,
    pub heading_1: Style,
    pub heading_2: Style,
    pub heading_3: Style,
    pub heading_4: Style,
    pub code_block: Style,
    pub assistant_prefix: Style,
    // Table
    pub table_header: Style,
    pub table_cell: Style,
    pub table_border: Style,
    // Popup
    pub popup_border: Style,
    pub popup_header: Style,
    pub popup_item: Style,
    pub popup_item_selected: Style,
    pub popup_item_desc: Style,
    pub popup_item_desc_selected: Style,
    pub popup_selected_bg: Style,
    pub popup_empty: Style,
    // Status
    pub status_help: Style,
    pub background: Style,
    pub text: Style,
}

impl Default for ConfigurableTheme {
    fn default() -> Self {
        let default = DefaultTheme;
        Self {
            border: default.border(),
            border_focused: default.border_focused(),
            help_text: default.help_text(),
            muted_text: default.muted_text(),
            focused_text: default.focused_text(),
            focus_indicator: default.focus_indicator(),
            selected: default.selected(),
            unselected: default.unselected(),
            button_confirm: default.button_confirm(),
            button_confirm_focused: default.button_confirm_focused(),
            button_cancel: default.button_cancel(),
            button_cancel_focused: default.button_cancel_focused(),
            warning: default.warning(),
            cursor: default.cursor(),
            category: default.category(),
            resource: default.resource(),
            // Markdown
            bold: default.bold(),
            italic: default.italic(),
            strikethrough: default.strikethrough(),
            inline_code: default.inline_code(),
            link_text: default.link_text(),
            link_url: default.link_url(),
            heading_1: default.heading_1(),
            heading_2: default.heading_2(),
            heading_3: default.heading_3(),
            heading_4: default.heading_4(),
            code_block: default.code_block(),
            assistant_prefix: default.assistant_prefix(),
            // Table
            table_header: default.table_header(),
            table_cell: default.table_cell(),
            table_border: default.table_border(),
            // Popup
            popup_border: default.popup_border(),
            popup_header: default.popup_header(),
            popup_item: default.popup_item(),
            popup_item_selected: default.popup_item_selected(),
            popup_item_desc: default.popup_item_desc(),
            popup_item_desc_selected: default.popup_item_desc_selected(),
            popup_selected_bg: default.popup_selected_bg(),
            popup_empty: default.popup_empty(),
            // Status
            status_help: default.status_help(),
            background: default.background(),
            text: default.text(),
        }
    }
}

impl Theme for ConfigurableTheme {
    fn border(&self) -> Style { self.border }
    fn border_focused(&self) -> Style { self.border_focused }
    fn help_text(&self) -> Style { self.help_text }
    fn muted_text(&self) -> Style { self.muted_text }
    fn focused_text(&self) -> Style { self.focused_text }
    fn focus_indicator(&self) -> Style { self.focus_indicator }
    fn selected(&self) -> Style { self.selected }
    fn unselected(&self) -> Style { self.unselected }
    fn button_confirm(&self) -> Style { self.button_confirm }
    fn button_confirm_focused(&self) -> Style { self.button_confirm_focused }
    fn button_cancel(&self) -> Style { self.button_cancel }
    fn button_cancel_focused(&self) -> Style { self.button_cancel_focused }
    fn warning(&self) -> Style { self.warning }
    fn cursor(&self) -> Style { self.cursor }
    fn category(&self) -> Style { self.category }
    fn resource(&self) -> Style { self.resource }
    // Markdown
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
    // Table
    fn table_header(&self) -> Style { self.table_header }
    fn table_cell(&self) -> Style { self.table_cell }
    fn table_border(&self) -> Style { self.table_border }
    // Popup
    fn popup_border(&self) -> Style { self.popup_border }
    fn popup_header(&self) -> Style { self.popup_header }
    fn popup_item(&self) -> Style { self.popup_item }
    fn popup_item_selected(&self) -> Style { self.popup_item_selected }
    fn popup_item_desc(&self) -> Style { self.popup_item_desc }
    fn popup_item_desc_selected(&self) -> Style { self.popup_item_desc_selected }
    fn popup_selected_bg(&self) -> Style { self.popup_selected_bg }
    fn popup_empty(&self) -> Style { self.popup_empty }
    // Status
    fn status_help(&self) -> Style { self.status_help }
    fn background(&self) -> Style { self.background }
    fn text(&self) -> Style { self.text }
}
