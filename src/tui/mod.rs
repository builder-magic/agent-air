//! LLM TUI Components
//!
//! Reusable Ratatui components for LLM-powered terminal applications.
//!
//! This module provides:
//!
//! - App - Complete TUI application with chat, input, and command handling
//! - ChatView - Chat message display with streaming support
//! - Layout system - Flexible layout system with templates and custom layouts
//! - Permission and question panels for tool interactions
//! - Markdown rendering with theming support
//! - Table rendering
//! - Session pickers
//! - Slash command system
//! - Text input with cursor management
//! - Theme system with 45+ built-in themes

mod app;
/// Slash command system.
pub mod commands;
/// Key handling and bindings.
pub mod keys;
/// Layout templates and providers.
pub mod layout;
/// Markdown rendering utilities.
pub mod markdown;
/// Table rendering utilities.
pub mod table;
/// Theme system and built-in themes.
pub mod themes;
/// TUI widgets (chat, input, panels).
pub mod widgets;

// Re-export App and related types
pub use app::{App, AppConfig};

// Re-export command system types
pub use commands::{
    // Standard commands
    ClearCommand,
    // Core types
    CommandContext,
    CommandRegistry,
    CommandResult,
    CompactCommand,
    CustomCommand,
    HelpCommand,
    NewSessionCommand,
    QuitCommand,
    SessionsCommand,
    SlashCommand,
    StatusCommand,
    ThemesCommand,
    VersionCommand,
    // Helper functions
    default_commands,
    filter_commands,
    generate_help_message,
    get_command_by_name,
    is_slash_command,
    parse_command,
};

// Re-export main types for convenience (now from widgets module)
pub use markdown::{
    ContentSegment, parse_to_spans, parse_to_styled_words, render_markdown_with_prefix,
    split_content_segments, wrap_with_prefix,
};
pub use table::{PulldownRenderer, TableRenderer, is_table_line, is_table_separator, render_table};
pub use widgets::{
    // Registerable widgets
    AnswerState,
    // Core widgets
    ChatView,
    // ConversationView trait and factory
    ConversationView,
    ConversationViewFactory,
    EnterAction,
    FocusItem,
    MessageRole,
    PermissionKeyAction,
    PermissionOption,
    PermissionPanel,
    QuestionKeyAction,
    QuestionPanel,
    RenderFn,
    SessionInfo,
    SessionPickerState,
    SimpleCommand,
    SlashCommandDisplay,
    SlashPopupState,
    StatusBar,
    StatusBarConfig,
    StatusBarData,
    TextInput,
    ToolMessageData,
    ToolStatus,
    render_session_picker,
    render_slash_popup,
};

// Re-export theme types for convenience
pub use themes::{
    THEMES, Theme, ThemeInfo, ThemePickerState, current_theme_name, default_theme_name, get_theme,
    init_theme, list_themes, render_theme_picker, set_theme, theme as app_theme,
};

// Re-export layout types for convenience
pub use layout::{
    LayoutContext, LayoutProvider, LayoutResult, LayoutTemplate, MinimalOptions, SidebarOptions,
    SidebarPosition, SidebarWidth, SplitOptions, SplitRatio, StandardOptions, WidgetSizes,
    helpers as layout_helpers,
};

// Re-export key handling types for convenience
pub use keys::{
    AppKeyAction, AppKeyResult, DefaultKeyHandler, ExitHandler, ExitState, KeyBindings, KeyCombo,
    KeyContext, KeyHandler,
};
