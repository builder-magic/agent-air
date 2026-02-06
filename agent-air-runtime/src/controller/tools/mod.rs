//! Tool execution framework with built-in tools.

mod ask_for_permissions;
mod ask_user_questions;
mod bash;
mod edit_file;
mod executor;
mod glob;
mod grep;
mod list_skills;
mod ls;
mod multi_edit;
mod read_file;
mod registry;
mod types;
mod user_interaction;
mod web_search;
mod write_file;

pub use ask_for_permissions::{
    ASK_FOR_PERMISSIONS_TOOL_DESCRIPTION, ASK_FOR_PERMISSIONS_TOOL_NAME,
    ASK_FOR_PERMISSIONS_TOOL_SCHEMA, AskForPermissionsTool,
};
pub use ask_user_questions::{
    ASK_USER_QUESTIONS_TOOL_DESCRIPTION, ASK_USER_QUESTIONS_TOOL_NAME,
    ASK_USER_QUESTIONS_TOOL_SCHEMA, Answer, AskUserQuestionsRequest, AskUserQuestionsResponse,
    AskUserQuestionsTool, Question, ValidationError, ValidationErrorCode, ValidationErrorDetail,
};
pub use executor::ToolExecutor;
pub use ls::{LS_TOOL_DESCRIPTION, LS_TOOL_NAME, LS_TOOL_SCHEMA, LsTool};
pub use read_file::{
    READ_FILE_TOOL_DESCRIPTION, READ_FILE_TOOL_NAME, READ_FILE_TOOL_SCHEMA, ReadFileTool,
};
pub use registry::{RegistryError, ToolRegistry};
pub use types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolBatchResult, ToolContext,
    ToolDefinition, ToolRequest, ToolResult, ToolResultStatus, ToolType,
};
// Re-export permission types from crate::permissions
pub use crate::client::models::Tool as LLMTool;
pub use crate::permissions::{
    PendingPermissionInfo, PermissionError, PermissionPanelResponse, PermissionRegistry,
};
pub use bash::{BASH_TOOL_DESCRIPTION, BASH_TOOL_NAME, BASH_TOOL_SCHEMA, BashTool};
pub use edit_file::{
    EDIT_FILE_TOOL_DESCRIPTION, EDIT_FILE_TOOL_NAME, EDIT_FILE_TOOL_SCHEMA, EditFileTool,
};
pub use glob::{GLOB_TOOL_DESCRIPTION, GLOB_TOOL_NAME, GLOB_TOOL_SCHEMA, GlobTool};
pub use grep::{
    GREP_TOOL_DESCRIPTION, GREP_TOOL_NAME, GREP_TOOL_SCHEMA, GrepTool, OutputMode as GrepOutputMode,
};
pub use list_skills::{
    LIST_SKILLS_TOOL_DESCRIPTION, LIST_SKILLS_TOOL_NAME, LIST_SKILLS_TOOL_SCHEMA, ListSkillsTool,
};
pub use multi_edit::{
    MULTI_EDIT_TOOL_DESCRIPTION, MULTI_EDIT_TOOL_NAME, MULTI_EDIT_TOOL_SCHEMA, MultiEditTool,
};
pub use user_interaction::{PendingQuestionInfo, UserInteractionError, UserInteractionRegistry};
pub use web_search::{
    WEB_SEARCH_TOOL_DESCRIPTION, WEB_SEARCH_TOOL_NAME, WEB_SEARCH_TOOL_SCHEMA, WebSearchTool,
};
pub use write_file::{
    WRITE_FILE_TOOL_DESCRIPTION, WRITE_FILE_TOOL_NAME, WRITE_FILE_TOOL_SCHEMA, WriteFileTool,
};
