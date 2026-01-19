use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::types::TurnId;
use vangogh_rs::models::Tool as VangoghTool;

/// Tool type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolType {
    ApiCall,
    BashCmd,
    WebSearch,
    TextEdit,
    Custom,
    FileRead,
    UserInteraction,
}

/// Context provided to tools during execution.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Session that requested the tool execution.
    pub session_id: i64,
    /// Unique ID for this tool use request.
    pub tool_use_id: String,
    /// Assistant turn ID.
    pub turn_id: Option<TurnId>,
}

/// Result status from tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolResultStatus {
    Success,
    Error,
    Timeout,
}

/// Content type for formatting tool results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultContentType {
    Json,
    Markdown,
    Yaml,
    #[default]
    PlainText,
    Xml,
    Auto,
}

/// Formatted display result from a tool.
#[derive(Debug, Clone)]
pub struct DisplayResult {
    /// The formatted content to display.
    pub content: String,
    /// How to render the content.
    pub content_type: ResultContentType,
    /// True if content was truncated.
    pub is_truncated: bool,
    /// Original line count for "50 of 200 lines" display.
    pub full_length: usize,
}

/// Configuration for how a tool should be displayed in the UI.
pub struct DisplayConfig {
    /// UI-friendly name (e.g., "Web Search" vs "web_search").
    pub display_name: String,
    /// Dynamic title based on input (e.g., "AWS/DynamoDB" for tool input).
    pub display_title: Box<dyn Fn(&HashMap<String, serde_json::Value>) -> String + Send + Sync>,
    /// Dynamic content based on input and result.
    pub display_content:
        Box<dyn Fn(&HashMap<String, serde_json::Value>, &str) -> DisplayResult + Send + Sync>,
}

impl DisplayConfig {
    /// Create a default display config for a tool.
    pub fn default_for(name: &str) -> Self {
        let name_owned = name.to_string();
        Self {
            display_name: name_owned,
            display_title: Box::new(|_| String::new()),
            display_content: Box::new(|_, result| DisplayResult {
                content: result.to_string(),
                content_type: ResultContentType::PlainText,
                is_truncated: false,
                full_length: result.lines().count(),
            }),
        }
    }
}

impl std::fmt::Display for ToolResultStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolResultStatus::Success => write!(f, "success"),
            ToolResultStatus::Error => write!(f, "error"),
            ToolResultStatus::Timeout => write!(f, "timeout"),
        }
    }
}

/// Result from a single tool execution.
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Session that requested the tool execution.
    pub session_id: i64,
    /// Tool name for registry lookup.
    pub tool_name: String,
    /// UI-friendly display name from tool's DisplayConfig.
    pub display_name: Option<String>,
    /// Links result to tool use request from LLM.
    pub tool_use_id: String,
    /// Original input parameters.
    pub input: HashMap<String, serde_json::Value>,
    /// Result content.
    pub content: String,
    /// Execution status.
    pub status: ToolResultStatus,
    /// Error message if status is Error.
    pub error: Option<String>,
    /// Assistant turn ID for this tool result.
    pub turn_id: Option<TurnId>,
    /// Pre-computed summary for compaction.
    pub compact_summary: Option<String>,
}

impl ToolResult {
    /// Create a successful tool result.
    pub fn success(
        session_id: i64,
        tool_name: String,
        display_name: Option<String>,
        tool_use_id: String,
        input: HashMap<String, serde_json::Value>,
        content: String,
        turn_id: Option<TurnId>,
        compact_summary: Option<String>,
    ) -> Self {
        Self {
            session_id,
            tool_name,
            display_name,
            tool_use_id,
            input,
            content,
            status: ToolResultStatus::Success,
            error: None,
            turn_id,
            compact_summary,
        }
    }

    /// Create an error tool result.
    pub fn error(
        session_id: i64,
        tool_name: String,
        tool_use_id: String,
        input: HashMap<String, serde_json::Value>,
        error: String,
        turn_id: Option<TurnId>,
    ) -> Self {
        let summary = format!("[{}: error]", tool_name);
        Self {
            session_id,
            tool_name,
            display_name: None,
            tool_use_id,
            input,
            content: String::new(),
            status: ToolResultStatus::Error,
            error: Some(error),
            turn_id,
            compact_summary: Some(summary),
        }
    }

    /// Create a timeout tool result.
    pub fn timeout(
        session_id: i64,
        tool_name: String,
        tool_use_id: String,
        input: HashMap<String, serde_json::Value>,
        turn_id: Option<TurnId>,
    ) -> Self {
        let summary = format!("[{}: timeout]", tool_name);
        Self {
            session_id,
            tool_name,
            display_name: None,
            tool_use_id,
            input,
            content: String::new(),
            status: ToolResultStatus::Timeout,
            error: Some("Tool execution timed out".to_string()),
            turn_id,
            compact_summary: Some(summary),
        }
    }
}

/// Batch result containing all results from parallel tool executions.
#[derive(Debug, Clone)]
pub struct ToolBatchResult {
    /// Unique batch identifier.
    pub batch_id: i64,
    /// Session that requested the batch.
    pub session_id: i64,
    /// Assistant turn ID for this batch.
    pub turn_id: Option<TurnId>,
    /// All results in original tool_use order.
    pub results: Vec<ToolResult>,
}

/// Request to execute a tool.
#[derive(Debug, Clone)]
pub struct ToolRequest {
    /// Tool use ID from the LLM.
    pub tool_use_id: String,
    /// Name of the tool to execute.
    pub tool_name: String,
    /// Input parameters.
    pub input: HashMap<String, serde_json::Value>,
}

/// Tool definition for the LLM.
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    /// Tool name (primary identifier).
    pub name: String,
    /// Tool description.
    pub description: String,
    /// JSON schema for input parameters.
    pub input_schema: String,
}

/// Trait for executable tools.
pub trait Executable: Send + Sync {
    /// Get the tool name.
    fn name(&self) -> &str;

    /// Get the tool description.
    fn description(&self) -> &str;

    /// Get the input schema as JSON string.
    fn input_schema(&self) -> &str;

    /// Get the tool type.
    fn tool_type(&self) -> ToolType;

    /// Execute the tool with given input.
    fn execute(
        &self,
        context: ToolContext,
        input: HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

    /// Convert to LLM tool definition.
    fn to_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: self.input_schema().to_string(),
        }
    }

    /// Convert to VangoghTool for provider APIs.
    fn to_vangogh_tool(&self) -> VangoghTool {
        VangoghTool::new(self.name(), self.description(), self.input_schema())
    }

    /// Get display configuration for UI rendering.
    fn display_config(&self) -> DisplayConfig {
        DisplayConfig::default_for(self.name())
    }

    /// Generate compact summary for context compaction.
    fn compact_summary(
        &self,
        _input: &HashMap<String, serde_json::Value>,
        _result: &str,
    ) -> String {
        format!("[{}: completed]", self.name())
    }
}
