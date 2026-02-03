//! ConversationView trait for conversation/chat display widgets.
//!
//! This trait defines the interface for widgets that display conversation history,
//! supporting streaming, tool messages, and session state management.

use ratatui::{layout::Rect, Frame};
use std::any::Any;

use super::ToolStatus;
use crate::themes::Theme;

/// Trait for conversation/chat display widgets.
///
/// This trait is separate from the Widget trait - a ConversationView may also
/// implement Widget, but this trait focuses specifically on conversation display
/// functionality.
///
/// # Session State
///
/// ConversationView supports saving and restoring state for session switching:
/// - `save_state()` returns an opaque state object
/// - `restore_state()` restores from a previously saved state
/// - `clear()` resets the view while preserving configuration
///
/// # Example Implementation
///
/// ```ignore
/// impl ConversationView for MyChatView {
///     fn add_user_message(&mut self, content: String) {
///         self.messages.push(Message::user(content));
///     }
///     // ... implement other methods
/// }
/// ```
pub trait ConversationView: Send + 'static {
    // --- Message Operations ---

    /// Add a user message to the conversation
    fn add_user_message(&mut self, content: String);

    /// Add an assistant message to the conversation
    fn add_assistant_message(&mut self, content: String);

    /// Add a system message to the conversation
    fn add_system_message(&mut self, content: String);

    // --- Streaming ---

    /// Append text to the current streaming response
    fn append_streaming(&mut self, text: &str);

    /// Complete the streaming response and finalize it as a message
    fn complete_streaming(&mut self);

    /// Discard the streaming buffer without saving (used on cancel)
    fn discard_streaming(&mut self);

    /// Check if currently streaming a response
    fn is_streaming(&self) -> bool;

    // --- Tool Messages ---

    /// Add a tool execution message
    fn add_tool_message(&mut self, tool_use_id: &str, display_name: &str, display_title: &str);

    /// Update the status of a tool message
    fn update_tool_status(&mut self, tool_use_id: &str, status: ToolStatus);

    // --- Scrolling ---

    /// Scroll up by the implementation's scroll step
    fn scroll_up(&mut self);

    /// Scroll down by the implementation's scroll step
    fn scroll_down(&mut self);

    /// Enable auto-scroll and scroll to bottom (called when user submits a message)
    fn enable_auto_scroll(&mut self);

    // --- Rendering ---

    /// Render the conversation view
    ///
    /// # Arguments
    /// * `frame` - The ratatui frame to render to
    /// * `area` - The area to render within
    /// * `theme` - The current theme
    /// * `pending_status` - Optional pending status message (e.g., "running tools...")
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, pending_status: Option<&str>);

    // --- Animation ---

    /// Advance the spinner animation (called periodically during activity)
    fn step_spinner(&mut self);

    // --- Session State ---

    /// Save the current state for session switching
    ///
    /// Returns an opaque state object that can be restored later.
    fn save_state(&self) -> Box<dyn Any + Send>;

    /// Restore state from a previously saved state
    ///
    /// If the state cannot be downcast to the expected type, this is a no-op.
    fn restore_state(&mut self, state: Box<dyn Any + Send>);

    /// Clear conversation content while preserving configuration
    ///
    /// This resets messages, streaming state, and scroll position,
    /// but keeps settings like title, theme configuration, and renderers.
    fn clear(&mut self);
}

/// Type alias for conversation view factory functions.
///
/// A factory is called to create a new ConversationView instance,
/// typically when creating a new session or clearing the current one.
///
/// # Example
///
/// ```ignore
/// let factory: ConversationViewFactory = Box::new(|| {
///     Box::new(ChatView::new()
///         .with_title("My Agent")
///         .with_initial_content(welcome_renderer))
/// });
/// ```
pub type ConversationViewFactory = Box<dyn Fn() -> Box<dyn ConversationView> + Send + Sync>;
