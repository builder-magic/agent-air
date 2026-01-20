// Doom Ayu Dark theme
// Ported from: https://github.com/doomemacs/themes

use ratatui::style::{Color, Modifier, Style};

use crate::tui::themes::theme::Theme;

const BG: Color = Color::Rgb(0x0d, 0x10, 0x17);
const BG_ALT: Color = Color::Rgb(0x13, 0x17, 0x21);
const FG: Color = Color::Rgb(0xbf, 0xbd, 0xb6);
const FG_ALT: Color = Color::Rgb(0x56, 0x5b, 0x66);
const BASE4: Color = Color::Rgb(0x56, 0x5b, 0x66);
const RED: Color = Color::Rgb(0xf0, 0x71, 0x78);
const ORANGE: Color = Color::Rgb(0xff, 0x8f, 0x40);
const YELLOW: Color = Color::Rgb(0xff, 0xb4, 0x54);
const GREEN: Color = Color::Rgb(0x7f, 0xd9, 0x62);
const CYAN: Color = Color::Rgb(0x39, 0xba, 0xe6);
const BLUE: Color = Color::Rgb(0x59, 0xc2, 0xff);
const MAGENTA: Color = Color::Rgb(0xd2, 0xa6, 0xff);
const VIOLET: Color = Color::Rgb(0xd2, 0xa6, 0xff);

pub fn theme() -> Theme {
    Theme {
        background: Style::default().bg(BG),
        text: Style::default().fg(FG),
        border: Style::default().fg(BASE4),
        border_focused: Style::default().fg(CYAN),
        title_separator: Style::default().fg(BASE4),
        title_indicator_connected: Style::default().fg(GREEN),
        title_indicator_disconnected: Style::default().fg(RED),
        title_text: Style::default().fg(FG),
        user_prefix: Style::default().fg(BLUE),
        system_prefix: Style::default().fg(YELLOW),
        assistant_prefix: Style::default().fg(MAGENTA),
        timestamp: Style::default().fg(FG_ALT),
        cursor: Style::default().fg(GREEN),
        bold: Modifier::BOLD,
        italic: Modifier::ITALIC,
        strikethrough: Modifier::CROSSED_OUT,
        inline_code: Style::default().fg(YELLOW),
        link_text: Style::default().fg(CYAN),
        link_url: Style::default().fg(VIOLET),
        heading_1: Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        heading_2: Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        heading_3: Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
        heading_4: Style::default().fg(FG).add_modifier(Modifier::BOLD),
        code_block: Style::default().fg(FG).bg(BG_ALT),
        table_header: Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        table_cell: Style::default().fg(FG),
        table_border: Style::default().fg(BASE4),
        tool_header: Style::default().fg(ORANGE),
        tool_executing: Style::default().fg(FG_ALT).add_modifier(Modifier::ITALIC),
        tool_completed: Style::default().fg(GREEN),
        tool_failed: Style::default().fg(RED),
        input_border: Style::default().fg(BASE4),
        input_text: Style::default().fg(FG),
        prompt: Style::default().fg(FG),
        throbber_label: Style::default().fg(FG_ALT),
        throbber_spinner: Style::default().fg(CYAN),
        status_help: Style::default().fg(FG_ALT),
        status_model: Style::default().fg(FG_ALT),
        popup_border: Style::default().fg(BASE4),
        popup_header: Style::default().fg(FG_ALT),
        popup_item: Style::default().fg(FG),
        popup_item_selected: Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        popup_item_desc: Style::default().fg(FG_ALT),
        popup_item_desc_selected: Style::default().fg(FG),
        popup_selected_bg: Style::default().bg(BG_ALT),
        popup_empty: Style::default().fg(FG_ALT).add_modifier(Modifier::ITALIC),
        // UI Panel styles - use theme-appropriate colors
        help_text: Style::default().fg(FG_ALT),
        muted_text: Style::default().fg(FG_ALT),
        focused_text: Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        focus_indicator: Style::default().fg(YELLOW),
        selected: Style::default().fg(GREEN),
        unselected: Style::default().fg(FG_ALT),
        button_confirm: Style::default().fg(GREEN),
        button_confirm_focused: Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        button_cancel: Style::default().fg(RED),
        button_cancel_focused: Style::default().fg(RED).add_modifier(Modifier::BOLD),
        warning: Style::default().fg(YELLOW),
        category: Style::default().fg(MAGENTA),
        resource: Style::default().fg(CYAN),
    }
}
