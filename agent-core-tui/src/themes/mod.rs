//! Application-level TUI components
//!
//! This module contains higher-level TUI components that build on the base primitives
//! in the parent `tui` module. These are shared across LLM agent applications.
//!
//! Components:
//! - Theme system with runtime switching
//! - Theme picker widget
//! - 45+ pre-built themes

#[macro_use]
/// Theme definition macros.
pub mod macros;
/// Theme runtime management.
pub mod theme;
/// Theme picker widget.
pub mod theme_picker;
/// Built-in theme definitions.
pub mod themes;

pub use theme::{current_theme_name, init_theme, set_theme, theme, Theme};
pub use theme_picker::{render_theme_picker, ThemePickerState};
pub use themes::{default_theme_name, get_theme, list_themes, ThemeInfo, THEMES};
