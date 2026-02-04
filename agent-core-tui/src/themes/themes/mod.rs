// Theme definitions for TUI
//
// Each theme module exports a Theme instance with colors ported from doom-emacs themes.

use crate::themes::theme::Theme;

crate::register_themes! {
    // Dark themes
    dark => { name: "dark", display: "Dark", dark: true },
    // Light themes
    light => { name: "light", display: "Light", dark: false },
}
