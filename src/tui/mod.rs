//! LLM TUI Components
//!
//! Reusable Ratatui components for LLM-powered terminal applications.
//!
//! This module provides UI widgets that work with the `llm-controller-rs` crate,
//! allowing multiple applications to share consistent UI patterns for:
//!
//! - Permission requests (file writes, deletes, network operations, etc.)
//! - Question/answer dialogs (AskUserQuestions tool)
//! - Markdown rendering with theming support
//! - Table rendering
//! - Session pickers
//! - Slash command popups
//! - Text input with cursor management

pub mod app_tui;
pub mod input;
pub mod markdown;
pub mod messages;
pub mod permission_panel;
pub mod question_panel;
pub mod session_picker;
pub mod slash_popup;
pub mod table;
pub mod theme;

// Re-export main types for convenience
pub use input::TextInput;
pub use markdown::{
    parse_to_spans, parse_to_styled_words, render_markdown_with_prefix, split_content_segments,
    wrap_with_prefix, ContentSegment,
};
pub use permission_panel::{
    KeyAction as PermissionKeyAction, PermissionOption, PermissionPanel,
};
pub use question_panel::{
    AnswerState, EnterAction, FocusItem, KeyAction as QuestionKeyAction, QuestionPanel,
};
pub use session_picker::{render_session_picker, SessionInfo, SessionPickerState};
pub use slash_popup::{render_slash_popup, SimpleCommand, SlashCommand, SlashPopupState};
pub use table::{is_table_line, is_table_separator, render_table, PulldownRenderer, TableRenderer};
pub use messages::{different_random_index, random_message_index, FUNNY_MESSAGES};
pub use theme::{ConfigurableTheme, DefaultTheme, Theme};

// Re-export app_tui types for convenience
pub use app_tui::{
    current_theme_name, default_theme_name, get_theme, init_theme, list_themes,
    render_theme_picker, set_theme, theme as app_theme, Theme as AppTheme,
    ThemeInfo, ThemePickerState, THEMES,
};
