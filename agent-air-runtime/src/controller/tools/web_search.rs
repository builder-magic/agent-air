//! Web Search tool implementation
//!
//! This is a wrapper around Claude's built-in web search tool.
//! The actual execution happens on Claude's side, so the exec function is a no-op.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use super::types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};

/// Web Search tool name constant.
pub const WEB_SEARCH_TOOL_NAME: &str = "web_search";

/// Web Search tool description constant.
pub const WEB_SEARCH_TOOL_DESCRIPTION: &str =
    "Performs web searches to find current information from the internet.";

/// Web Search tool JSON schema constant.
pub const WEB_SEARCH_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "query": {
            "type": "string",
            "description": "The search query"
        }
    },
    "required": ["query"]
}"#;

/// Tool that performs web searches using Claude's built-in capability.
pub struct WebSearchTool;

impl WebSearchTool {
    /// Create a new WebSearchTool instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Executable for WebSearchTool {
    fn name(&self) -> &str {
        WEB_SEARCH_TOOL_NAME
    }

    fn description(&self) -> &str {
        WEB_SEARCH_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        WEB_SEARCH_TOOL_SCHEMA
    }

    fn tool_type(&self) -> ToolType {
        ToolType::WebSearch
    }

    fn execute(
        &self,
        _context: ToolContext,
        _input: HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        // This is a no-op because Claude handles web search natively.
        // We never actually run this - it's just to satisfy the Executable trait.
        Box::pin(async move {
            // This should never be called as Claude handles web search natively
            Ok(String::new())
        })
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Web Search".to_string(),
            display_title: Box::new(|input| {
                input
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            }),
            display_content: Box::new(|_input, _result| {
                // Web search results are handled by citations, return minimal content
                DisplayResult {
                    content: "Search completed (results shown via citations)".to_string(),
                    content_type: ResultContentType::PlainText,
                    is_truncated: false,
                    full_length: 0,
                }
            }),
        }
    }

    fn compact_summary(&self, input: &HashMap<String, serde_json::Value>, _result: &str) -> String {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        format!("[WebSearch: {}]", query)
    }
}
