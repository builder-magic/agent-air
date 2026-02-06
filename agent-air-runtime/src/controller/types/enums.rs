// Enum types for the LLM controller

use std::fmt;

/// Type of request being sent to the LLM
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LLMRequestType {
    /// A user message request
    UserMessage,
    /// A tool result request
    ToolResult,
}

impl fmt::Display for LLMRequestType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UserMessage => write!(f, "user_message"),
            Self::ToolResult => write!(f, "tool_result"),
        }
    }
}

/// Type of response from the LLM
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LLMResponseType {
    /// Stream has started (contains message_id, model)
    StreamStart,
    /// A text chunk from streaming response
    TextChunk,
    /// A tool use block has started (contains id, name)
    ToolUseStart,
    /// Incremental JSON for tool input
    ToolInputDelta,
    /// A single tool use request (complete)
    ToolUse,
    /// A batch of tool use requests
    ToolBatch,
    /// Response is complete
    Complete,
    /// An error occurred
    Error,
    /// Token usage update
    TokenUpdate,
}

impl fmt::Display for LLMResponseType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StreamStart => write!(f, "stream_start"),
            Self::TextChunk => write!(f, "text_chunk"),
            Self::ToolUseStart => write!(f, "tool_use_start"),
            Self::ToolInputDelta => write!(f, "tool_input_delta"),
            Self::ToolUse => write!(f, "tool_use"),
            Self::ToolBatch => write!(f, "tool_batch"),
            Self::Complete => write!(f, "complete"),
            Self::Error => write!(f, "error"),
            Self::TokenUpdate => write!(f, "token_update"),
        }
    }
}

/// Type of input to the controller
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    /// A control command
    Control,
    /// User data/message
    Data,
}

impl fmt::Display for InputType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Control => write!(f, "ip_control"),
            Self::Data => write!(f, "ip_data"),
        }
    }
}

/// Control commands for the controller
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCmd {
    /// Interrupt current operation
    Interrupt,
    /// Shutdown the controller
    Shutdown,
    /// Clear conversation history for a session
    Clear,
    /// Trigger compaction for a session
    Compact,
}

impl fmt::Display for ControlCmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Interrupt => write!(f, "cc_interrupt"),
            Self::Shutdown => write!(f, "cc_shutdown"),
            Self::Clear => write!(f, "cc_clear"),
            Self::Compact => write!(f, "cc_compact"),
        }
    }
}

/// Type of content block in a message
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentBlockType {
    /// Plain text content
    Text,
    /// Tool use request
    ToolUse,
    /// Tool result response
    ToolResult,
}

impl fmt::Display for ContentBlockType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::ToolUse => write!(f, "tool_use"),
            Self::ToolResult => write!(f, "tool_result"),
        }
    }
}

/// Role of a message in the conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    /// User message
    User,
    /// Assistant message
    Assistant,
}

impl fmt::Display for MessageRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
        }
    }
}

impl MessageRole {
    /// Returns the role as a string slice
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_request_type_display() {
        assert_eq!(LLMRequestType::UserMessage.to_string(), "user_message");
        assert_eq!(LLMRequestType::ToolResult.to_string(), "tool_result");
    }

    #[test]
    fn test_llm_response_type_display() {
        assert_eq!(LLMResponseType::TextChunk.to_string(), "text_chunk");
        assert_eq!(LLMResponseType::Complete.to_string(), "complete");
    }

    #[test]
    fn test_message_role() {
        assert_eq!(MessageRole::User.as_str(), "user");
        assert_eq!(MessageRole::Assistant.to_string(), "assistant");
    }
}
