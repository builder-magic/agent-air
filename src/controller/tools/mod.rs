//! Tool execution framework with built-in tools.

mod ask_for_permissions;
mod ask_user_questions;
mod executor;
mod grep;
mod ls;
mod permission_registry;
mod read_file;
mod registry;
mod types;
mod user_interaction;
mod web_search;
mod write_file;

pub use ask_for_permissions::{
    AskForPermissionsTool, PermissionCategory, PermissionRequest, PermissionResponse,
    PermissionScope, ASK_FOR_PERMISSIONS_TOOL_DESCRIPTION, ASK_FOR_PERMISSIONS_TOOL_NAME,
    ASK_FOR_PERMISSIONS_TOOL_SCHEMA,
};
pub use ask_user_questions::{
    Answer, AskUserQuestionsRequest, AskUserQuestionsResponse, AskUserQuestionsTool, Question,
    ValidationError, ValidationErrorCode, ValidationErrorDetail, ASK_USER_QUESTIONS_TOOL_DESCRIPTION,
    ASK_USER_QUESTIONS_TOOL_NAME, ASK_USER_QUESTIONS_TOOL_SCHEMA,
};
pub use executor::ToolExecutor;
pub use ls::{LsTool, LS_TOOL_DESCRIPTION, LS_TOOL_NAME, LS_TOOL_SCHEMA};
pub use read_file::{
    ReadFileTool, READ_FILE_TOOL_DESCRIPTION, READ_FILE_TOOL_NAME, READ_FILE_TOOL_SCHEMA,
};
pub use registry::{RegistryError, ToolRegistry};
pub use types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolBatchResult, ToolContext,
    ToolDefinition, ToolRequest, ToolResult, ToolResultStatus, ToolType,
};
pub use permission_registry::{PendingPermissionInfo, PermissionError, PermissionGrant, PermissionRegistry};
pub use user_interaction::{PendingQuestionInfo, UserInteractionError, UserInteractionRegistry};
pub use crate::client::models::Tool as LLMTool;
pub use web_search::{
    WebSearchTool, WEB_SEARCH_TOOL_DESCRIPTION, WEB_SEARCH_TOOL_NAME, WEB_SEARCH_TOOL_SCHEMA,
};
pub use write_file::{
    WriteFileTool, WRITE_FILE_TOOL_DESCRIPTION, WRITE_FILE_TOOL_NAME, WRITE_FILE_TOOL_SCHEMA,
};
pub use grep::{
    GrepTool, OutputMode as GrepOutputMode, GREP_TOOL_DESCRIPTION, GREP_TOOL_NAME,
    GREP_TOOL_SCHEMA,
};
