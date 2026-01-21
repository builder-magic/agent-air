mod error;
mod llm_controller;
pub mod session;
pub mod stateless;
pub mod tools;
pub mod types;
pub mod usage;

pub use error::ControllerError;
pub use llm_controller::{EventFunc, LLMController, DEFAULT_CHANNEL_SIZE};
pub use session::{
    CompactResult, CompactionConfig, LLMProvider, LLMSession, LLMSessionConfig, LLMSessionManager,
    SessionStatus, TokenUsage, ToolCompaction,
};
pub use stateless::{
    RequestOptions, StatelessConfig, StatelessError, StatelessExecutor, StatelessResult,
};
pub use tools::{
    Answer, AskForPermissionsTool, AskUserQuestionsRequest, AskUserQuestionsResponse,
    AskUserQuestionsTool, DisplayConfig, DisplayResult, Executable, PendingPermissionInfo,
    PendingQuestionInfo, PermissionCategory, PermissionError, PermissionGrant, PermissionRegistry,
    PermissionRequest, PermissionResponse, PermissionScope, Question, ResultContentType,
    ToolBatchResult, ToolContext, ToolDefinition, ToolExecutor, ToolRegistry, ToolRequest,
    ToolResult, ToolResultStatus, ToolType, UserInteractionError, UserInteractionRegistry,
    ValidationError, ValidationErrorCode, ValidationErrorDetail, LLMTool, WebSearchTool,
    ASK_FOR_PERMISSIONS_TOOL_DESCRIPTION, ASK_FOR_PERMISSIONS_TOOL_NAME,
    ASK_FOR_PERMISSIONS_TOOL_SCHEMA, ASK_USER_QUESTIONS_TOOL_DESCRIPTION,
    ASK_USER_QUESTIONS_TOOL_NAME, ASK_USER_QUESTIONS_TOOL_SCHEMA, WEB_SEARCH_TOOL_DESCRIPTION,
    WEB_SEARCH_TOOL_NAME, WEB_SEARCH_TOOL_SCHEMA,
};
pub use types::{
    ContentBlock, ControlCmd, ControllerEvent, ControllerInputPayload, FromLLMPayload, InputType,
    LLMRequestType, LLMResponseType, Message, MessageRole, ToLLMPayload, TurnCounter, TurnId,
};
pub use usage::{TokenMeter, TokenUsageTracker};
