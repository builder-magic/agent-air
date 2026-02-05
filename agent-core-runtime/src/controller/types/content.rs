// Content block types for messages

use std::collections::HashMap;

use super::ContentBlockType;

/// A content block within a message
#[derive(Debug, Clone, PartialEq)]
pub enum ContentBlock {
    /// Plain text content
    Text(TextBlock),
    /// Tool use request from the assistant
    ToolUse(ToolUseBlock),
    /// Tool result from tool execution
    ToolResult(ToolResultBlock),
}

impl ContentBlock {
    /// Returns the type of this content block
    pub fn block_type(&self) -> ContentBlockType {
        match self {
            Self::Text(_) => ContentBlockType::Text,
            Self::ToolUse(_) => ContentBlockType::ToolUse,
            Self::ToolResult(_) => ContentBlockType::ToolResult,
        }
    }

    /// Creates a new text block
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(TextBlock { text: text.into() })
    }

    /// Creates a new tool use block
    pub fn tool_use(
        id: impl Into<String>,
        name: impl Into<String>,
        input: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self::ToolUse(ToolUseBlock {
            id: id.into(),
            name: name.into(),
            input,
        })
    }

    /// Creates a new tool result block
    pub fn tool_result(
        tool_use_id: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self::ToolResult(ToolResultBlock {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error,
            compact_summary: None,
        })
    }
}

/// A text content block
#[derive(Debug, Clone, PartialEq)]
pub struct TextBlock {
    /// The text content
    pub text: String,
}

/// A tool use request block
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUseBlock {
    /// Unique identifier for this tool use
    pub id: String,
    /// Name of the tool to use
    pub name: String,
    /// Input parameters for the tool
    pub input: HashMap<String, serde_json::Value>,
}

/// A tool result block
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResultBlock {
    /// ID of the corresponding tool use
    pub tool_use_id: String,
    /// Result content (typically JSON)
    pub content: String,
    /// Whether this result represents an error
    pub is_error: bool,
    /// Pre-computed summary for compaction
    pub compact_summary: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_block() {
        let block = ContentBlock::text("Hello, world!");
        assert_eq!(block.block_type(), ContentBlockType::Text);

        if let ContentBlock::Text(text_block) = block {
            assert_eq!(text_block.text, "Hello, world!");
        } else {
            panic!("Expected text block");
        }
    }

    #[test]
    fn test_tool_use_block() {
        let mut input = HashMap::new();
        input.insert("query".to_string(), serde_json::json!("test"));

        let block = ContentBlock::tool_use("tool_123", "search", input);
        assert_eq!(block.block_type(), ContentBlockType::ToolUse);

        if let ContentBlock::ToolUse(tool_block) = block {
            assert_eq!(tool_block.id, "tool_123");
            assert_eq!(tool_block.name, "search");
        } else {
            panic!("Expected tool use block");
        }
    }

    #[test]
    fn test_tool_result_block() {
        let block = ContentBlock::tool_result("tool_123", "{\"result\": \"success\"}", false);
        assert_eq!(block.block_type(), ContentBlockType::ToolResult);

        if let ContentBlock::ToolResult(result_block) = block {
            assert_eq!(result_block.tool_use_id, "tool_123");
            assert!(!result_block.is_error);
        } else {
            panic!("Expected tool result block");
        }
    }
}
