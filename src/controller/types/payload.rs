// Payload types for controller communication

use std::collections::HashMap;

use super::{ControlCmd, InputType, LLMRequestType, LLMResponseType, TurnId};
use crate::controller::tools::{AskUserQuestionsRequest, PermissionRequest, ToolResultStatus};
use crate::permissions::BatchPermissionRequest;

/// Payload for requests sent to the LLM
#[derive(Debug, Clone)]
pub struct ToLLMPayload {
    /// Type of request
    pub request_type: LLMRequestType,
    /// The message content (for UserMessage requests)
    pub content: String,
    /// Tool results to send back (for ToolResult requests)
    pub tool_results: Vec<ToolResultInfo>,
    /// Optional per-request options (model, max_tokens, etc.)
    pub options: Option<LLMRequestOptions>,
    /// Assistant turn ID for this request
    pub turn_id: Option<TurnId>,
    /// Tool use ID to compact summary mapping
    pub compact_summaries: HashMap<String, String>,
}

/// Per-request LLM options that override session defaults
#[derive(Debug, Clone, Default)]
pub struct LLMRequestOptions {
    /// Model to use for this request
    pub model: Option<String>,
    /// Maximum tokens for the response
    pub max_tokens: Option<i64>,
    /// System prompt override
    pub system_prompt: Option<String>,
}

/// Payload for responses from the LLM
#[derive(Debug, Clone)]
pub struct FromLLMPayload {
    /// Session ID this response belongs to
    pub session_id: i64,
    /// Type of response
    pub response_type: LLMResponseType,
    /// Text content (for TextChunk responses)
    pub text: String,
    /// Single tool use (for ToolUse responses)
    pub tool_use: Option<ToolUseInfo>,
    /// Batch of tool uses (for ToolBatch responses)
    pub tool_uses: Vec<ToolUseInfo>,
    /// Whether the response is complete
    pub is_complete: bool,
    /// Error message if response_type is Error
    pub error: Option<String>,
    /// Model used for this response
    pub model: String,
    /// Message ID for streaming (from StreamStart)
    pub message_id: String,
    /// Content block index (for streaming deltas)
    pub content_index: usize,
    /// Stop reason when complete
    pub stop_reason: Option<String>,
    /// Input tokens used
    pub input_tokens: i64,
    /// Output tokens generated
    pub output_tokens: i64,
    /// Assistant turn ID
    pub turn_id: Option<TurnId>,
}

impl Default for FromLLMPayload {
    fn default() -> Self {
        Self {
            session_id: 0,
            response_type: LLMResponseType::TextChunk,
            text: String::new(),
            tool_use: None,
            tool_uses: Vec::new(),
            is_complete: false,
            error: None,
            model: String::new(),
            message_id: String::new(),
            content_index: 0,
            stop_reason: None,
            input_tokens: 0,
            output_tokens: 0,
            turn_id: None,
        }
    }
}

/// Information about a tool use request
#[derive(Debug, Clone)]
pub struct ToolUseInfo {
    /// Unique ID for this tool use
    pub id: String,
    /// Name of the tool
    pub name: String,
    /// Input parameters as JSON
    pub input: serde_json::Value,
}

/// Information about a tool result to send back to the LLM
#[derive(Debug, Clone)]
pub struct ToolResultInfo {
    /// ID of the tool use this result corresponds to
    pub tool_use_id: String,
    /// Result content
    pub content: String,
    /// Whether this result represents an error
    pub is_error: bool,
}

/// Payload for input to the controller
#[derive(Debug, Clone)]
pub struct ControllerInputPayload {
    /// Type of input
    pub input_type: InputType,
    /// Session ID this input is for
    pub session_id: i64,
    /// Content (for Data input type)
    pub content: String,
    /// Control command (for Control input type)
    pub control_cmd: Option<ControlCmd>,
    /// Assistant turn ID
    pub turn_id: Option<TurnId>,
}

impl ControllerInputPayload {
    /// Creates a new data input payload
    pub fn data(session_id: i64, content: impl Into<String>, turn_id: TurnId) -> Self {
        Self {
            input_type: InputType::Data,
            session_id,
            content: content.into(),
            control_cmd: None,
            turn_id: Some(turn_id),
        }
    }

    /// Creates a new control input payload
    pub fn control(session_id: i64, cmd: ControlCmd) -> Self {
        Self {
            input_type: InputType::Control,
            session_id,
            content: String::new(),
            control_cmd: Some(cmd),
            turn_id: None,
        }
    }
}

/// Event emitted by the controller
#[derive(Debug, Clone)]
pub enum ControllerEvent {
    /// Streaming has started
    StreamStart {
        session_id: i64,
        message_id: String,
        model: String,
        turn_id: Option<TurnId>,
    },
    /// Text chunk from LLM response
    TextChunk {
        session_id: i64,
        text: String,
        turn_id: Option<TurnId>,
    },
    /// Tool use block has started (streaming)
    ToolUseStart {
        session_id: i64,
        tool_id: String,
        tool_name: String,
        turn_id: Option<TurnId>,
    },
    /// Tool use requested by LLM (complete)
    ToolUse {
        session_id: i64,
        tool: ToolUseInfo,
        /// UI-friendly display name from tool's DisplayConfig
        display_name: Option<String>,
        /// Dynamic title based on input (e.g., "Seattle, WA" for weather)
        display_title: Option<String>,
        turn_id: Option<TurnId>,
    },
    /// Response complete
    Complete {
        session_id: i64,
        stop_reason: Option<String>,
        turn_id: Option<TurnId>,
    },
    /// Error occurred
    Error {
        session_id: i64,
        error: String,
        turn_id: Option<TurnId>,
    },
    /// Token usage update
    TokenUpdate {
        session_id: i64,
        input_tokens: i64,
        output_tokens: i64,
        context_limit: i32,
    },
    /// Individual tool execution result (for UI feedback during batch execution)
    ToolResult {
        session_id: i64,
        tool_use_id: String,
        tool_name: String,
        /// UI-friendly display name from tool's DisplayConfig
        display_name: Option<String>,
        status: ToolResultStatus,
        content: String,
        error: Option<String>,
        turn_id: Option<TurnId>,
    },
    /// Control command completed
    CommandComplete {
        session_id: i64,
        command: ControlCmd,
        success: bool,
        message: Option<String>,
    },
    /// Tool is blocked waiting for user interaction (questions)
    UserInteractionRequired {
        session_id: i64,
        tool_use_id: String,
        request: AskUserQuestionsRequest,
        turn_id: Option<TurnId>,
    },
    /// Tool is blocked waiting for permission from user
    PermissionRequired {
        session_id: i64,
        tool_use_id: String,
        request: PermissionRequest,
        turn_id: Option<TurnId>,
    },
    /// Multiple tools are blocked waiting for batch permission from user.
    /// This is used when parallel tools need permissions - presenting all requests
    /// together avoids deadlocks and allows the user to make informed decisions.
    BatchPermissionRequired {
        session_id: i64,
        batch: BatchPermissionRequest,
        turn_id: Option<TurnId>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_input_data() {
        let input = ControllerInputPayload::data(1, "Hello", TurnId::new_user_turn(1));
        assert_eq!(input.input_type, InputType::Data);
        assert_eq!(input.session_id, 1);
        assert_eq!(input.content, "Hello");
        assert!(input.control_cmd.is_none());
        assert!(input.turn_id.is_some());
    }

    #[test]
    fn test_controller_input_control() {
        let input = ControllerInputPayload::control(1, ControlCmd::Interrupt);
        assert_eq!(input.input_type, InputType::Control);
        assert_eq!(input.session_id, 1);
        assert_eq!(input.control_cmd, Some(ControlCmd::Interrupt));
        assert!(input.turn_id.is_none());
    }

    #[test]
    fn test_from_llm_payload_default() {
        let payload = FromLLMPayload::default();
        assert_eq!(payload.session_id, 0);
        assert_eq!(payload.response_type, LLMResponseType::TextChunk);
        assert!(!payload.is_complete);
    }
}
