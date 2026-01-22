/// Message role in a conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    /// System message providing instructions or context.
    System,
    /// User message containing requests or responses.
    User,
    /// Assistant message containing responses or tool use.
    Assistant,
}

/// Image source types supported by LLM providers.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageSource {
    /// Base64-encoded image data with media type (e.g., "image/jpeg", "image/png").
    Base64 {
        media_type: String,
        data: String,
    },
    /// URL reference to an image.
    Url(String),
}

/// Tool invocation requested by the assistant.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUse {
    /// Unique identifier for this tool use (used to match with ToolResult).
    pub id: String,
    /// Name of the tool to invoke.
    pub name: String,
    /// Input arguments as a JSON string.
    pub input: String,
}

/// Result of a tool invocation, sent back to the assistant.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    /// ID of the tool use this result corresponds to.
    pub tool_use_id: String,
    /// Result content (can be text or error message).
    pub content: String,
    /// Whether this result represents an error.
    pub is_error: bool,
}

/// Content block types that can appear in messages.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// Plain text content.
    Text(String),
    /// Image content with source data.
    Image(ImageSource),
    /// Tool invocation from the assistant.
    ToolUse(ToolUse),
    /// Tool result from the user.
    ToolResult(ToolResult),
}

/// Conversation message with role and content blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Role of the message sender.
    pub role: Role,
    /// Content blocks (text, images, tool use, tool results).
    pub content: Vec<Content>,
}

impl Message {
    /// Create a new message with a single text content block.
    pub fn new(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![Content::Text(text.into())],
        }
    }

    /// Create a message with multiple content blocks.
    pub fn with_content(role: Role, content: Vec<Content>) -> Self {
        Self { role, content }
    }

    /// Create a system message with text content.
    pub fn system(text: impl Into<String>) -> Self {
        Self::new(Role::System, text)
    }

    /// Create a user message with text content.
    pub fn user(text: impl Into<String>) -> Self {
        Self::new(Role::User, text)
    }

    /// Create an assistant message with text content.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::new(Role::Assistant, text)
    }

    /// Create a user message with a tool result.
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self {
            role: Role::User,
            content: vec![Content::ToolResult(ToolResult {
                tool_use_id: tool_use_id.into(),
                content: content.into(),
                is_error,
            })],
        }
    }
}

/// Tool definition for function calling.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    /// Name of the tool.
    pub name: String,
    /// Description of what the tool does.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: String,
}

impl Tool {
    /// Create a new tool definition.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema: input_schema.into(),
        }
    }
}

/// Controls how the model uses tools.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolChoice {
    /// Model decides whether to use tools.
    Auto,
    /// Model must use at least one tool.
    Any,
    /// Model must use the specified tool.
    Tool(String),
    /// Model cannot use any tools.
    None,
}

impl Default for ToolChoice {
    fn default() -> Self {
        Self::Auto
    }
}

/// Metadata for request tracking.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Metadata {
    /// User identifier for tracking/billing.
    pub user_id: Option<String>,
}

/// Options for LLM message requests.
#[derive(Debug, Clone, Default)]
pub struct MessageOptions {
    /// Sampling temperature (0.0-1.0).
    pub temperature: Option<f32>,
    /// Maximum tokens to generate.
    pub max_tokens: Option<u32>,
    /// Model to use.
    pub model: Option<String>,
    /// Tools available for the model to use.
    pub tools: Option<Vec<Tool>>,
    /// How the model should use tools.
    pub tool_choice: Option<ToolChoice>,
    /// Custom stop sequences.
    pub stop_sequences: Option<Vec<String>>,
    /// Nucleus sampling parameter.
    pub top_p: Option<f32>,
    /// Top-K sampling parameter.
    pub top_k: Option<u32>,
    /// Request metadata.
    pub metadata: Option<Metadata>,
}

// ============================================================================
// Streaming Types
// ============================================================================

/// Events emitted during streaming responses.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// Stream started, contains message metadata.
    MessageStart {
        message_id: String,
        model: String,
    },
    /// A content block is starting.
    ContentBlockStart {
        index: usize,
        block_type: ContentBlockType,
    },
    /// Incremental text content.
    TextDelta {
        index: usize,
        text: String,
    },
    /// Incremental JSON for tool input.
    InputJsonDelta {
        index: usize,
        json: String,
    },
    /// A content block has finished.
    ContentBlockStop {
        index: usize,
    },
    /// Message-level updates (stop reason, usage).
    MessageDelta {
        stop_reason: Option<String>,
        usage: Option<Usage>,
    },
    /// Stream has ended.
    MessageStop,
    /// Keep-alive ping.
    Ping,
}

/// Type of content block in streaming.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentBlockType {
    Text,
    ToolUse { id: String, name: String },
}

/// Token usage statistics.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Usage {
    /// Input tokens used.
    pub input_tokens: u32,
    /// Output tokens generated.
    pub output_tokens: u32,
}