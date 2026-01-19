// Doom Nord theme
// Ported from: https://github.com/doomemacs/themes

use ratatui::style::{Color, Modifier, Style};

use crate::tui::app_tui::theme::Theme;

// Color palette
const BG: Color = Color::Rgb(0x2E, 0x34, 0x40);
const BG_ALT: Color = Color::Rgb(0x27, 0x2C, 0x36);
const FG: Color = Color::Rgb(0xEC, 0xEF, 0xF4);
const FG_ALT: Color = Color::Rgb(0x4C, 0x56, 0x6A);
const BASE4: Color = Color::Rgb(0x43, 0x4C, 0x5E);
const RED: Color = Color::Rgb(0xBF, 0x61, 0x6A);
const ORANGE: Color = Color::Rgb(0xD0, 0x87, 0x70);
const YELLOW: Color = Color::Rgb(0xEB, 0xCB, 0x8B);
const GREEN: Color = Color::Rgb(0xA3, 0xBE, 0x8C);
const CYAN: Color = Color::Rgb(0x88, 0xC0, 0xD0);
const BLUE: Color = Color::Rgb(0x81, 0xA1, 0xC1);
const MAGENTA: Color = Color::Rgb(0xB4, 0x8E, 0xAD);
const VIOLET: Color = Color::Rgb(0x5E, 0x81, 0xAC);

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
    }
}
