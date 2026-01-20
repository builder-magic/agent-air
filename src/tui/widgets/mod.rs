//! TUI Widgets
//!
//! Reusable widget components for LLM-powered terminal applications.
//!
//! # Core Widgets (always present)
//! - [`ChatView`] - Chat message display with streaming support
//! - [`TextInput`] - Text input buffer with cursor management
//!
//! # Registerable Widgets
//! - [`PermissionPanel`] - Permission request panel for tool interactions
//! - [`QuestionPanel`] - Question panel for user input collection
//! - [`SessionPickerState`] - Session picker for viewing/switching sessions
//! - [`SlashPopupState`] - Slash command popup
//!
//! # Widget System
//!
//! Widgets can be registered with the App via the [`Widget`] trait. This allows
//! agents to customize which widgets are available.

use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
use std::any::Any;

use crate::controller::{
    AskUserQuestionsRequest, AskUserQuestionsResponse, PermissionRequest, PermissionResponse,
    TurnId,
};
use crate::tui::themes::Theme;

pub mod chat;
pub mod input;
pub mod permission_panel;
pub mod question_panel;
pub mod session_picker;
pub mod slash_popup;

pub use chat::{ChatView, ChatViewConfig, MessageRole, ToolMessageData, ToolStatus};
pub use input::TextInput;
pub use permission_panel::{KeyAction as PermissionKeyAction, PermissionOption, PermissionPanel};
pub use question_panel::{
    AnswerState, EnterAction, FocusItem, KeyAction as QuestionKeyAction, QuestionPanel,
};
pub use session_picker::{render_session_picker, SessionInfo, SessionPickerState};
pub use slash_popup::{
    render_slash_popup, SimpleCommand, SlashCommand as SlashCommandTrait, SlashPopupState,
};

/// Standard widget IDs for built-in widgets
pub mod widget_ids {
    // Core widgets (always present)
    pub const CHAT_VIEW: &str = "chat_view";
    pub const TEXT_INPUT: &str = "text_input";

    // Registerable widgets
    pub const PERMISSION_PANEL: &str = "permission_panel";
    pub const QUESTION_PANEL: &str = "question_panel";
    pub const SESSION_PICKER: &str = "session_picker";
    pub const SLASH_POPUP: &str = "slash_popup";
    pub const THEME_PICKER: &str = "theme_picker";
}

/// Result of a widget handling a key event
#[derive(Debug, Clone)]
pub enum WidgetKeyResult {
    /// Key was not handled by this widget
    NotHandled,
    /// Key was handled, no further action needed
    Handled,
    /// Key was handled, and the widget requests an action from App
    Action(WidgetAction),
}

/// Actions that widgets can request from the App
#[derive(Debug, Clone)]
pub enum WidgetAction {
    /// Submit a question panel response
    SubmitQuestion {
        tool_use_id: String,
        response: AskUserQuestionsResponse,
    },
    /// Cancel a question panel
    CancelQuestion { tool_use_id: String },
    /// Submit a permission panel response
    SubmitPermission {
        tool_use_id: String,
        response: PermissionResponse,
    },
    /// Cancel a permission panel
    CancelPermission { tool_use_id: String },
    /// Switch to a different session
    SwitchSession { session_id: i64 },
    /// Execute a slash command
    ExecuteCommand { command: String },
    /// Close the widget (theme picker confirm/cancel)
    Close,
}

/// Trait for registerable TUI widgets
///
/// Widgets that implement this trait can be registered with the App and will
/// receive key events and rendering opportunities based on their state.
pub trait Widget: Send + 'static {
    /// Unique identifier for this widget type
    fn id(&self) -> &'static str;

    /// Priority for key event handling (higher = checked first)
    ///
    /// Modal widgets should have high priority to intercept input.
    /// Default is 100.
    fn priority(&self) -> u8 {
        100
    }

    /// Whether the widget is currently active/visible
    fn is_active(&self) -> bool;

    /// Handle key event, return result indicating what action to take
    fn handle_key(&mut self, key: KeyEvent, theme: &Theme) -> WidgetKeyResult;

    /// Render the widget
    fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme);

    /// Calculate required height for this widget
    ///
    /// Returns 0 if the widget doesn't need dedicated space.
    fn required_height(&self, available: u16) -> u16 {
        let _ = available;
        0
    }

    /// Whether this widget blocks input to the text input when active
    fn blocks_input(&self) -> bool {
        false
    }

    /// Whether this widget is a full-screen overlay
    ///
    /// Overlay widgets are rendered on top of everything else.
    fn is_overlay(&self) -> bool {
        false
    }

    /// Cast to Any for downcasting
    fn as_any(&self) -> &dyn Any;

    /// Cast to Any for mutable downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Convert to Box<dyn Any> for owned downcasting
    fn into_any(self: Box<Self>) -> Box<dyn Any>;

    // --- Activation methods ---
    // These have default no-op implementations. Specific widgets override.

    /// Activate for a permission request
    fn activate_permission(
        &mut self,
        _tool_use_id: String,
        _session_id: i64,
        _request: PermissionRequest,
        _turn_id: Option<TurnId>,
    ) {
    }

    /// Activate for a user question request
    fn activate_question(
        &mut self,
        _tool_use_id: String,
        _session_id: i64,
        _request: AskUserQuestionsRequest,
        _turn_id: Option<TurnId>,
    ) {
    }

    /// Activate session picker with sessions
    fn activate_sessions(&mut self, _sessions: Vec<SessionInfo>, _current_session_id: i64) {}

    /// Activate theme picker
    fn activate_theme(&mut self, _current_theme_name: &str, _current_theme: Theme) {}

    /// Deactivate the widget
    fn deactivate(&mut self) {}

    /// Get additional context for slash popup (filtered command count)
    fn set_slash_context(&mut self, _filtered_count: usize) {}

    /// Activate slash popup
    fn activate_slash(&mut self) {}
}
