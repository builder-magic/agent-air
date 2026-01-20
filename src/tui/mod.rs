//! LLM TUI Components
//!
//! Reusable Ratatui components for LLM-powered terminal applications.
//!
//! This module provides:
//!
//! - [`App`] - Complete TUI application with chat, input, and command handling
//! - [`ChatView`] - Chat message display with streaming support
//! - Permission and question panels for tool interactions
//! - Markdown rendering with theming support
//! - Table rendering
//! - Session pickers
//! - Slash command system
//! - Text input with cursor management
//! - Theme system with 45+ built-in themes

mod app;
pub mod themes;
mod chat;
mod commands;
pub mod input;
pub mod markdown;
pub mod messages;
pub mod table;
pub mod widgets;

// Re-export App and related types
pub use app::{App, AppConfig, AppMode, EXIT_MODE_TIMEOUT_SECS};
pub use chat::{ChatView, ChatViewConfig, MessageRole, ToolMessageData, ToolStatus};
pub use commands::{
    filter_commands, generate_help_message, get_command_by_name, get_default_commands,
    is_slash_command, parse_command, SlashCommand, DEFAULT_COMMANDS,
};

// Re-export main types for convenience
pub use input::TextInput;
pub use markdown::{
    parse_to_spans, parse_to_styled_words, render_markdown_with_prefix, split_content_segments,
    wrap_with_prefix, ContentSegment,
};
pub use widgets::{
    AnswerState, EnterAction, FocusItem, PermissionKeyAction, PermissionOption, PermissionPanel,
    QuestionKeyAction, QuestionPanel, SessionInfo, SessionPickerState, SimpleCommand,
    SlashCommandTrait, SlashPopupState, render_session_picker, render_slash_popup,
};
pub use table::{is_table_line, is_table_separator, render_table, PulldownRenderer, TableRenderer};
pub use messages::{different_random_index, random_message_index, FUNNY_MESSAGES};

// Re-export theme types for convenience
pub use themes::{
    current_theme_name, default_theme_name, get_theme, init_theme, list_themes,
    render_theme_picker, set_theme, theme as app_theme, Theme, ThemeInfo,
    ThemePickerState, THEMES,
};
