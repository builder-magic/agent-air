// Message types for TUI-Controller communication
//
// These types are used to send events from the LLM controller to the TUI
// and from the TUI to the controller.

use crate::controller::types::ControlCmd;
use crate::controller::{AskUserQuestionsRequest, ToolResultStatus, TurnId};
use crate::permissions::{BatchPermissionRequest, PermissionRequest};

/// Messages sent from the controller to the TUI for display
#[derive(Debug, Clone)]
pub enum UiMessage {
    /// Streaming text chunk from LLM response
    TextChunk {
        session_id: i64,
        turn_id: Option<TurnId>,
        text: String,
        input_tokens: i64,
        output_tokens: i64,
    },

    /// General display message (tool status, info, etc.)
    Display {
        session_id: i64,
        turn_id: Option<TurnId>,
        message: String,
    },

    /// Tool execution has started
    ToolExecuting {
        session_id: i64,
        turn_id: Option<TurnId>,
        tool_use_id: String,
        display_name: String,
        display_title: String,
    },

    /// Tool execution has completed
    ToolCompleted {
        session_id: i64,
        turn_id: Option<TurnId>,
        tool_use_id: String,
        status: ToolResultStatus,
        error: Option<String>,
    },

    /// LLM response is complete
    Complete {
        session_id: i64,
        turn_id: Option<TurnId>,
        input_tokens: i64,
        output_tokens: i64,
        /// Stop reason from LLM - "tool_use" means tools will execute, "end_turn" means done
        stop_reason: Option<String>,
    },

    /// Real-time token count update
    TokenUpdate {
        session_id: i64,
        turn_id: Option<TurnId>,
        input_tokens: i64,
        output_tokens: i64,
        context_limit: i32,
    },

    /// Error from the controller
    Error {
        session_id: i64,
        turn_id: Option<TurnId>,
        error: String,
    },

    /// System message (TUI-local, not from controller)
    System { session_id: i64, message: String },

    /// Control command completed
    CommandComplete {
        session_id: i64,
        command: ControlCmd,
        success: bool,
        message: Option<String>,
    },

    /// Tool is blocked waiting for user interaction (e.g., AskUserQuestions)
    UserInteractionRequired {
        session_id: i64,
        tool_use_id: String,
        request: AskUserQuestionsRequest,
        turn_id: Option<TurnId>,
    },

    /// Tool is blocked waiting for permission (e.g., AskForPermissions)
    PermissionRequired {
        session_id: i64,
        tool_use_id: String,
        request: PermissionRequest,
        turn_id: Option<TurnId>,
    },

    /// Multiple tools are blocked waiting for batch permission.
    /// This is used when parallel tools need permissions - presenting all requests
    /// together avoids deadlocks and allows the user to make informed decisions.
    BatchPermissionRequired {
        session_id: i64,
        batch: BatchPermissionRequest,
        turn_id: Option<TurnId>,
    },
}

/// Channel type aliases for TUI-Controller communication
pub mod channels {
    use crate::controller::ControllerInputPayload;
    use tokio::sync::mpsc;

    use super::UiMessage;

    // Re-export from controller for backwards compatibility
    pub use crate::controller::DEFAULT_CHANNEL_SIZE;

    /// Sender for messages from TUI to controller
    pub type ToControllerTx = mpsc::Sender<ControllerInputPayload>;
    /// Receiver for messages from TUI to controller
    pub type ToControllerRx = mpsc::Receiver<ControllerInputPayload>;

    /// Sender for messages from controller to TUI
    pub type FromControllerTx = mpsc::Sender<UiMessage>;
    /// Receiver for messages from controller to TUI
    pub type FromControllerRx = mpsc::Receiver<UiMessage>;

    /// Creates a pair of channels for TUI-Controller communication
    ///
    /// # Arguments
    /// * `channel_size` - Optional buffer size for channels. Uses DEFAULT_CHANNEL_SIZE if None.
    pub fn create_channels(
        channel_size: Option<usize>,
    ) -> (
        ToControllerTx,
        ToControllerRx,
        FromControllerTx,
        FromControllerRx,
    ) {
        let size = channel_size.unwrap_or(DEFAULT_CHANNEL_SIZE);
        let (to_ctrl_tx, to_ctrl_rx) = mpsc::channel(size);
        let (from_ctrl_tx, from_ctrl_rx) = mpsc::channel(size);
        (to_ctrl_tx, to_ctrl_rx, from_ctrl_tx, from_ctrl_rx)
    }
}
