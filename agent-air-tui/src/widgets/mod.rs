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
//! Widgets can be registered with the App via the Widget trait. This allows
//! agents to customize which widgets are available.

use chrono::{DateTime, Local};
use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Rect};
use std::any::Any;

use crate::controller::{AskUserQuestionsResponse, PermissionPanelResponse};
use crate::keys::NavigationHelper;
use crate::permissions::BatchPermissionResponse;
use crate::themes::Theme;

pub mod batch_permission_panel;
pub mod chat;
pub mod chat_helpers;
pub mod conversation;
pub mod input;
pub mod onboarding;
pub mod permission_panel;
pub mod question_panel;
pub mod select_panel;
pub mod session_picker;
pub mod slash_popup;
pub mod status_bar;

pub use batch_permission_panel::{
    BatchKeyAction, BatchPermissionOption, BatchPermissionPanel, BatchPermissionPanelConfig,
};
pub use chat::{ChatView, ChatViewConfig, MessageRole, ToolMessageData, ToolStatus};
pub use chat_helpers::RenderFn;
pub use conversation::{ConversationView, ConversationViewFactory};
pub use input::TextInput;
pub use onboarding::{
    OnboardingConfig, OnboardingModel, OnboardingProvider, OnboardingResult, OnboardingWidget,
    default_providers, write_config_yaml,
};
pub use permission_panel::{
    KeyAction as PermissionKeyAction, PermissionOption, PermissionPanel, PermissionPanelConfig,
};
pub use question_panel::{
    AnswerState, EnterAction, FocusItem, KeyAction as QuestionKeyAction, QuestionPanel,
    QuestionPanelConfig,
};
pub use select_panel::{SelectItem, SelectPanel, SelectPanelConfig};
pub use session_picker::{
    SessionInfo, SessionPickerConfig, SessionPickerState, render_session_picker,
};
pub use slash_popup::{
    SimpleCommand, SlashCommandDisplay, SlashPopupConfig, SlashPopupState, render_slash_popup,
};
pub use status_bar::{StatusBar, StatusBarConfig, StatusBarData};

/// Information about a registered LLM provider (for status display).
#[derive(Clone, Debug)]
pub struct AppProviderInfo {
    /// Provider name (e.g. "anthropic", "openai").
    pub name: String,
    /// Whether this is the default provider.
    pub is_default: bool,
}

/// Generic status data assembled by App and passed to overlay widgets via
/// [`Widget::prepare_overlay`]. This avoids the framework needing to know
/// about concrete overlay types.
#[derive(Clone, Debug)]
pub struct AppStatusData {
    /// Current session ID.
    pub session_id: i64,
    /// Model name.
    pub model: String,
    /// Tokens used in context.
    pub context_used: i64,
    /// Maximum context limit.
    pub context_limit: i32,
    /// Session creation timestamp.
    pub created_at: DateTime<Local>,
    /// Agent display name.
    pub agent_name: String,
    /// Agent version string.
    pub agent_version: String,
    /// Total number of active sessions.
    pub total_sessions: usize,
    /// Registered providers.
    pub providers: Vec<AppProviderInfo>,
}

/// Standard widget IDs for built-in widgets
pub mod widget_ids {
    // Core widgets (always present)
    pub const CHAT_VIEW: &str = "chat_view";
    pub const TEXT_INPUT: &str = "text_input";

    // Registerable widgets
    pub const BATCH_PERMISSION_PANEL: &str = "batch_permission_panel";
    pub const ONBOARDING: &str = "onboarding";
    pub const PERMISSION_PANEL: &str = "permission_panel";
    pub const QUESTION_PANEL: &str = "question_panel";
    pub const SESSION_PICKER: &str = "session_picker";
    pub const SLASH_POPUP: &str = "slash_popup";
    pub const THEME_PICKER: &str = "theme_picker";
    pub const STATUS_BAR: &str = "status_bar";
    pub const STATUS_PANE: &str = "status_pane";
    pub const SELECT_PANEL: &str = "select_panel";
}

/// Context provided to widgets when handling key events.
///
/// This contains references to the theme and a navigation helper that allows
/// widgets to respect configured key bindings instead of hardcoding key codes.
pub struct WidgetKeyContext<'a> {
    /// Theme for styling.
    pub theme: &'a Theme,
    /// Navigation helper for checking key bindings.
    pub nav: NavigationHelper<'a>,
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
        response: PermissionPanelResponse,
    },
    /// Cancel a permission panel
    CancelPermission { tool_use_id: String },
    /// Submit a batch permission response
    SubmitBatchPermission {
        batch_id: String,
        response: BatchPermissionResponse,
    },
    /// Cancel a batch permission panel
    CancelBatchPermission { batch_id: String },
    /// Switch to a different session
    SwitchSession { session_id: i64 },
    /// Execute a slash command
    ExecuteCommand { command: String },
    /// Close the widget (theme picker confirm/cancel)
    Close,
    /// A generic single-select picker ([`SelectPanel`]) confirmed a choice.
    ///
    /// `picker_id` identifies the logical picker (e.g. "model") so the App can
    /// route the result; `item_id` is the id of the chosen item.
    SelectItem {
        picker_id: String,
        item_id: String,
    },
    /// Onboarding wizard completed — register provider and create session.
    CompleteOnboarding {
        provider_id: String,
        model_id: String,
        api_key: String,
    },
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

    /// Handle key event, return result indicating what action to take.
    ///
    /// The `ctx` parameter provides access to the theme and a navigation helper
    /// that respects configured key bindings.
    fn handle_key(&mut self, key: KeyEvent, ctx: &WidgetKeyContext) -> WidgetKeyResult;

    /// Render the widget
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme);

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

    /// Prepare and activate the widget as an overlay.
    ///
    /// Called by App when the widget should display as an overlay.
    /// The `data` argument carries generic status data (e.g. `AppStatusData`)
    /// that the widget can downcast to extract what it needs.
    fn prepare_overlay(&mut self, _data: &dyn Any) {}

    /// Cast to Any for downcasting
    fn as_any(&self) -> &dyn Any;

    /// Cast to Any for mutable downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Convert to `Box<dyn Any>` for owned downcasting
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}
