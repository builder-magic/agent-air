// Macros for theme definition and registration
//
// These macros reduce boilerplate when defining themes by generating
// the standard theme function from a color palette.

/// Macro to define a theme from a color palette.
/// Generates the `pub fn theme() -> Theme` function.
///
/// # Example
/// ```ignore
/// define_theme! {
///     bg: (0x28, 0x2a, 0x36),
///     bg_alt: (0x1E, 0x20, 0x29),
///     fg: (0xf8, 0xf8, 0xf2),
///     fg_alt: (0x62, 0x72, 0xa4),
///     base4: (0x56, 0x57, 0x61),
///     red: (0xff, 0x55, 0x55),
///     orange: (0xff, 0xb8, 0x6c),
///     yellow: (0xf1, 0xfa, 0x8c),
///     green: (0x50, 0xfa, 0x7b),
///     cyan: (0x8b, 0xe9, 0xfd),
///     blue: (0x61, 0xbf, 0xff),
///     magenta: (0xff, 0x79, 0xc6),
///     violet: (0xbd, 0x93, 0xf9)
/// }
/// ```
#[macro_export]
macro_rules! define_theme {
    (
        bg: ($bg_r:expr, $bg_g:expr, $bg_b:expr),
        bg_alt: ($bg_alt_r:expr, $bg_alt_g:expr, $bg_alt_b:expr),
        fg: ($fg_r:expr, $fg_g:expr, $fg_b:expr),
        fg_alt: ($fg_alt_r:expr, $fg_alt_g:expr, $fg_alt_b:expr),
        base4: ($base4_r:expr, $base4_g:expr, $base4_b:expr),
        red: ($red_r:expr, $red_g:expr, $red_b:expr),
        orange: ($orange_r:expr, $orange_g:expr, $orange_b:expr),
        yellow: ($yellow_r:expr, $yellow_g:expr, $yellow_b:expr),
        green: ($green_r:expr, $green_g:expr, $green_b:expr),
        cyan: ($cyan_r:expr, $cyan_g:expr, $cyan_b:expr),
        blue: ($blue_r:expr, $blue_g:expr, $blue_b:expr),
        magenta: ($magenta_r:expr, $magenta_g:expr, $magenta_b:expr),
        violet: ($violet_r:expr, $violet_g:expr, $violet_b:expr)
    ) => {
        pub fn theme() -> Theme {
            use ratatui::style::{Color, Modifier, Style};

            const BG: Color = Color::Rgb($bg_r, $bg_g, $bg_b);
            const BG_ALT: Color = Color::Rgb($bg_alt_r, $bg_alt_g, $bg_alt_b);
            const FG: Color = Color::Rgb($fg_r, $fg_g, $fg_b);
            const FG_ALT: Color = Color::Rgb($fg_alt_r, $fg_alt_g, $fg_alt_b);
            const BASE4: Color = Color::Rgb($base4_r, $base4_g, $base4_b);
            const RED: Color = Color::Rgb($red_r, $red_g, $red_b);
            const ORANGE: Color = Color::Rgb($orange_r, $orange_g, $orange_b);
            const YELLOW: Color = Color::Rgb($yellow_r, $yellow_g, $yellow_b);
            const GREEN: Color = Color::Rgb($green_r, $green_g, $green_b);
            const CYAN: Color = Color::Rgb($cyan_r, $cyan_g, $cyan_b);
            const BLUE: Color = Color::Rgb($blue_r, $blue_g, $blue_b);
            const MAGENTA: Color = Color::Rgb($magenta_r, $magenta_g, $magenta_b);
            const VIOLET: Color = Color::Rgb($violet_r, $violet_g, $violet_b);

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
    };
}

/// Macro to register all themes with their metadata.
/// Generates mod declarations, THEMES array, and get_theme function.
///
/// # Example
/// ```ignore
/// register_themes! {
///     dracula => { name: "dracula", display: "Dracula", dark: true },
///     one => { name: "one", display: "One", dark: true },
/// }
/// ```
#[macro_export]
macro_rules! register_themes {
    (
        $(
            $module:ident => {
                name: $name:literal,
                display: $display:literal,
                dark: $is_dark:expr
            }
        ),* $(,)?
    ) => {
        // Generate mod declarations
        $(mod $module;)*

        /// Theme metadata for display in picker
        pub struct ThemeInfo {
            pub name: &'static str,
            pub display_name: &'static str,
            pub is_dark: bool,
        }

        /// All available themes
        pub const THEMES: &[ThemeInfo] = &[
            $(
                ThemeInfo {
                    name: $name,
                    display_name: $display,
                    is_dark: $is_dark,
                },
            )*
        ];

        /// Get a theme by name
        pub fn get_theme(name: &str) -> Option<Theme> {
            match name {
                $($name => Some($module::theme()),)*
                _ => None,
            }
        }

        /// List all available theme names
        pub fn list_themes() -> Vec<&'static str> {
            THEMES.iter().map(|t| t.name).collect()
        }

        /// Get default theme name
        pub fn default_theme_name() -> &'static str {
            "dark"
        }
    };
}

// Re-export macros at module level for use within the crate
pub use define_theme;
pub use register_themes;
