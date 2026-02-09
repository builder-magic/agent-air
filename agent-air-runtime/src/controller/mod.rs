//! LLM session controller and tool execution framework.

mod error;
mod llm_controller;
/// Session management and context compaction.
pub mod session;
/// Stateless execution support.
pub mod stateless;
/// Tool execution and user interaction.
pub mod tools;
/// Core types for controller communication.
pub mod types;
/// Token usage tracking.
pub mod usage;

pub use error::ControllerError;
pub use llm_controller::{DEFAULT_CHANNEL_SIZE, LLMController};
pub use session::{
    CompactResult, CompactionConfig, LLMProvider, LLMSession, LLMSessionConfig, LLMSessionManager,
    SessionStatus, TokenUsage, ToolCompaction,
};
pub use stateless::{
    RequestOptions, StatelessConfig, StatelessError, StatelessExecutor, StatelessResult,
};
pub use tools::{
    ASK_FOR_PERMISSIONS_TOOL_DESCRIPTION, ASK_FOR_PERMISSIONS_TOOL_NAME,
    ASK_FOR_PERMISSIONS_TOOL_SCHEMA, ASK_USER_QUESTIONS_TOOL_DESCRIPTION,
    ASK_USER_QUESTIONS_TOOL_NAME, ASK_USER_QUESTIONS_TOOL_SCHEMA, Answer, AskForPermissionsTool,
    AskUserQuestionsRequest, AskUserQuestionsResponse, AskUserQuestionsTool, BASH_TOOL_DESCRIPTION,
    BASH_TOOL_NAME, BASH_TOOL_SCHEMA, BashTool, DisplayConfig, DisplayResult,
    EDIT_FILE_TOOL_DESCRIPTION, EDIT_FILE_TOOL_NAME, EDIT_FILE_TOOL_SCHEMA, EditFileTool,
    Executable, GLOB_TOOL_DESCRIPTION, GLOB_TOOL_NAME, GLOB_TOOL_SCHEMA, GREP_TOOL_DESCRIPTION,
    GREP_TOOL_NAME, GREP_TOOL_SCHEMA, GlobTool, GrepOutputMode, GrepTool,
    LIST_PLANS_TOOL_DESCRIPTION, LIST_PLANS_TOOL_NAME, LIST_PLANS_TOOL_SCHEMA,
    LIST_SKILLS_TOOL_DESCRIPTION, LIST_SKILLS_TOOL_NAME, LIST_SKILLS_TOOL_SCHEMA, LLMTool,
    LS_TOOL_DESCRIPTION, LS_TOOL_NAME, LS_TOOL_SCHEMA, ListPlansTool, ListSkillsTool, LsTool,
    MARKDOWN_PLAN_TOOL_DESCRIPTION, MARKDOWN_PLAN_TOOL_NAME, MARKDOWN_PLAN_TOOL_SCHEMA,
    MULTI_EDIT_TOOL_DESCRIPTION, MULTI_EDIT_TOOL_NAME, MULTI_EDIT_TOOL_SCHEMA, MarkdownPlanTool,
    MultiEditTool, PendingPermissionInfo, PendingQuestionInfo, PermissionError,
    PermissionPanelResponse, PermissionRegistry, PlanStore, Question, READ_FILE_TOOL_DESCRIPTION,
    READ_FILE_TOOL_NAME, READ_FILE_TOOL_SCHEMA, READ_PLAN_TOOL_DESCRIPTION, READ_PLAN_TOOL_NAME,
    READ_PLAN_TOOL_SCHEMA, ReadFileTool, ReadPlanTool, ResultContentType, ToolBatchResult,
    ToolContext, ToolDefinition, ToolExecutor, ToolRegistry, ToolRequest, ToolResult,
    ToolResultStatus, ToolType, UPDATE_PLAN_STEP_TOOL_DESCRIPTION, UPDATE_PLAN_STEP_TOOL_NAME,
    UPDATE_PLAN_STEP_TOOL_SCHEMA, UpdatePlanStepTool, UserInteractionError,
    UserInteractionRegistry, ValidationError, ValidationErrorCode, ValidationErrorDetail,
    WEB_SEARCH_TOOL_DESCRIPTION, WEB_SEARCH_TOOL_NAME, WEB_SEARCH_TOOL_SCHEMA,
    WRITE_FILE_TOOL_DESCRIPTION, WRITE_FILE_TOOL_NAME, WRITE_FILE_TOOL_SCHEMA, WebSearchTool,
    WriteFileTool,
};
// Re-export new permission types
pub use crate::permissions::{Grant, GrantTarget, PermissionLevel, PermissionRequest};
pub use types::{
    ContentBlock, ControlCmd, ControllerEvent, ControllerInputPayload, FromLLMPayload, InputType,
    LLMRequestType, LLMResponseType, Message, MessageRole, ToLLMPayload, TurnCounter, TurnId,
};
pub use usage::{TokenMeter, TokenUsageTracker};
