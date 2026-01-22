//! LLM TUI Components
//!
//! Reusable Ratatui components for LLM-powered terminal applications.
//!
//! This module provides:
//!
//! - [`App`] - Complete TUI application with chat, input, and command handling
//! - [`ChatView`] - Chat message display with streaming support
//! - [`layout`] - Flexible layout system with templates and custom layouts
//! - Permission and question panels for tool interactions
//! - Markdown rendering with theming support
//! - Table rendering
//! - Session pickers
//! - Slash command system
//! - Text input with cursor management
//! - Theme system with 45+ built-in themes

mod app;
pub mod commands;
pub mod keys;
pub mod layout;
pub mod markdown;
pub mod table;
pub mod themes;
pub mod widgets;

// Re-export App and related types
pub use app::{App, AppConfig};

// Re-export command system types
pub use commands::{
    // Core types
    CommandContext, CommandRegistry, CommandResult, CustomCommand, SlashCommand,
    // Standard commands
    ClearCommand, CompactCommand, HelpCommand, NewSessionCommand, QuitCommand,
    SessionsCommand, StatusCommand, ThemesCommand, VersionCommand,
    // Helper functions
    default_commands, filter_commands, generate_help_message, get_command_by_name,
    is_slash_command, parse_command,
};

// Re-export main types for convenience (now from widgets module)
pub use widgets::{
    // Core widgets
    ChatView, MessageRole, TextInput, ToolMessageData, ToolStatus, RenderFn,
    // ConversationView trait and factory
    ConversationView, ConversationViewFactory,
    // Registerable widgets
    AnswerState, EnterAction, FocusItem, PermissionKeyAction, PermissionOption, PermissionPanel,
    QuestionKeyAction, QuestionPanel, SessionInfo, SessionPickerState, SimpleCommand,
    SlashCommandDisplay, SlashPopupState, render_session_picker, render_slash_popup,
};
pub use markdown::{
    parse_to_spans, parse_to_styled_words, render_markdown_with_prefix, split_content_segments,
    wrap_with_prefix, ContentSegment,
};
pub use table::{is_table_line, is_table_separator, render_table, PulldownRenderer, TableRenderer};

// Re-export theme types for convenience
pub use themes::{
    current_theme_name, default_theme_name, get_theme, init_theme, list_themes,
    render_theme_picker, set_theme, theme as app_theme, Theme, ThemeInfo,
    ThemePickerState, THEMES,
};

// Re-export layout types for convenience
pub use layout::{
    LayoutContext, LayoutProvider, LayoutResult, LayoutTemplate,
    MinimalOptions, SidebarOptions, SidebarPosition, SidebarWidth,
    SplitOptions, SplitRatio, StandardOptions, WidgetSizes,
    helpers as layout_helpers,
};

// Re-export key handling types for convenience
pub use keys::{
    AppKeyAction, AppKeyResult, DefaultKeyHandler, ExitHandler, ExitState,
    KeyBindings, KeyCombo, KeyContext, KeyHandler,
};
