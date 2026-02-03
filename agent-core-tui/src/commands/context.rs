//! Command execution context.

use std::any::Any;

use tokio::sync::mpsc;

use crate::controller::{ControlCmd, ControllerInputPayload};
use crate::widgets::ConversationView;

use super::SlashCommand;

/// Actions that require App-level handling after command returns.
#[derive(Debug, Clone)]
pub enum PendingAction {
    /// Open the theme picker overlay.
    OpenThemePicker,
    /// Open the session picker overlay.
    OpenSessionPicker,
    /// Clear the conversation view.
    ClearConversation,
    /// Compact the conversation history.
    CompactConversation,
    /// Create a new session.
    CreateNewSession,
    /// Quit the application.
    Quit,
}

/// Context provided to commands during execution.
///
/// Provides controlled access to app functionality through methods rather
/// than exposing internal state directly.
///
/// # Extension Data
///
/// Agents can provide custom extension data that commands can access:
///
/// ```ignore
/// // In agent setup:
/// agent.set_command_extension(MyContext { api_key: "..." });
///
/// // In command execution:
/// fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
///     let my_ctx = ctx.extension::<MyContext>().expect("MyContext required");
///     // use my_ctx.api_key
/// }
/// ```
pub struct CommandContext<'a> {
    // Base context
    session_id: i64,
    agent_name: &'a str,
    version: &'a str,
    commands: &'a [Box<dyn SlashCommand>],

    // Conversation view for displaying messages
    conversation: &'a mut dyn ConversationView,

    // Channel to send commands to controller
    controller_tx: Option<&'a mpsc::Sender<ControllerInputPayload>>,

    // Pending actions to execute after command returns
    pending_actions: Vec<PendingAction>,

    // Agent-provided extension data
    extension: Option<&'a dyn Any>,
}

impl<'a> CommandContext<'a> {
    /// Create a new command context.
    ///
    /// This is called internally by App when executing commands.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        session_id: i64,
        agent_name: &'a str,
        version: &'a str,
        commands: &'a [Box<dyn SlashCommand>],
        conversation: &'a mut dyn ConversationView,
        controller_tx: Option<&'a mpsc::Sender<ControllerInputPayload>>,
        extension: Option<&'a dyn Any>,
    ) -> Self {
        Self {
            session_id,
            agent_name,
            version,
            commands,
            conversation,
            controller_tx,
            pending_actions: Vec::new(),
            extension,
        }
    }

    // --- Base context accessors ---

    /// Get the current session ID.
    pub fn session_id(&self) -> i64 {
        self.session_id
    }

    /// Get the agent name.
    pub fn agent_name(&self) -> &str {
        self.agent_name
    }

    /// Get the agent version.
    pub fn version(&self) -> &str {
        self.version
    }

    /// Get all registered commands (useful for /help).
    pub fn commands(&self) -> &[Box<dyn SlashCommand>] {
        self.commands
    }

    // --- Conversation operations ---

    /// Display a system message in the conversation.
    pub fn show_message(&mut self, msg: impl Into<String>) {
        self.conversation.add_system_message(msg.into());
    }

    // --- Deferred actions (handled by App after execute returns) ---

    /// Request to clear the conversation.
    ///
    /// This is deferred until after the command returns.
    pub fn clear_conversation(&mut self) {
        self.pending_actions.push(PendingAction::ClearConversation);
    }

    /// Request to compact the conversation history.
    ///
    /// This is deferred until after the command returns.
    pub fn compact_conversation(&mut self) {
        self.pending_actions.push(PendingAction::CompactConversation);
    }

    /// Request to open the theme picker.
    ///
    /// This is deferred until after the command returns.
    pub fn open_theme_picker(&mut self) {
        self.pending_actions.push(PendingAction::OpenThemePicker);
    }

    /// Request to open the session picker.
    ///
    /// This is deferred until after the command returns.
    pub fn open_session_picker(&mut self) {
        self.pending_actions.push(PendingAction::OpenSessionPicker);
    }

    /// Request to quit the application.
    ///
    /// This is deferred until after the command returns.
    pub fn request_quit(&mut self) {
        self.pending_actions.push(PendingAction::Quit);
    }

    /// Request to create a new session.
    ///
    /// This is deferred until after the command returns.
    pub fn create_new_session(&mut self) {
        self.pending_actions.push(PendingAction::CreateNewSession);
    }

    // --- Controller communication ---

    /// Send a control command to the controller.
    ///
    /// This allows commands to interact with the LLM controller,
    /// for example to create a new session.
    pub fn send_to_controller(&self, cmd: ControlCmd) {
        if let Some(tx) = self.controller_tx {
            let payload = ControllerInputPayload::control(self.session_id, cmd);
            let _ = tx.try_send(payload);
        }
    }

    // --- Extension access ---

    /// Get agent-specific extension data.
    ///
    /// Returns `None` if no extension was provided or if the type doesn't match.
    ///
    /// # Example
    ///
    /// ```ignore
    /// struct MyContext { api_key: String }
    ///
    /// fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
    ///     if let Some(my_ctx) = ctx.extension::<MyContext>() {
    ///         // Use my_ctx.api_key
    ///     }
    /// }
    /// ```
    pub fn extension<T: 'static>(&self) -> Option<&T> {
        self.extension?.downcast_ref::<T>()
    }

    // --- Internal: get pending actions ---

    /// Take the pending actions (internal use by App).
    pub(crate) fn take_pending_actions(&mut self) -> Vec<PendingAction> {
        std::mem::take(&mut self.pending_actions)
    }
}
