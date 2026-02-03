// Type conversion between controller types and LLM client types

use crate::client::models::{
    Content as LLMContent, Message as LLMMessage, Role as LLMRole,
    ToolResult as LLMToolResult, ToolUse as LLMToolUse,
};

use crate::controller::types::{ContentBlock, Message, MessageRole, TextBlock, ToolResultBlock, ToolUseBlock};

/// Convert our Message to LLM client Message
pub fn to_llm_message(msg: &Message) -> LLMMessage {
    let role = match msg.role() {
        MessageRole::User => LLMRole::User,
        MessageRole::Assistant => LLMRole::Assistant,
    };

    let content: Vec<LLMContent> = msg
        .content()
        .iter()
        .filter_map(|block| to_llm_content(block))
        .collect();

    LLMMessage::with_content(role, content)
}

/// Convert our ContentBlock to LLM client Content
pub fn to_llm_content(block: &ContentBlock) -> Option<LLMContent> {
    match block {
        ContentBlock::Text(text_block) => Some(LLMContent::Text(text_block.text.clone())),
        ContentBlock::ToolUse(tool_use) => {
            Some(LLMContent::ToolUse(LLMToolUse {
                id: tool_use.id.clone(),
                name: tool_use.name.clone(),
                // Convert HashMap<String, Value> to JSON string
                input: serde_json::to_string(&tool_use.input).unwrap_or_default(),
            }))
        }
        ContentBlock::ToolResult(tool_result) => {
            Some(LLMContent::ToolResult(LLMToolResult {
                tool_use_id: tool_result.tool_use_id.clone(),
                content: tool_result.content.clone(),
                is_error: tool_result.is_error,
            }))
        }
    }
}

/// Convert LLM client Message to our ContentBlock list
pub fn from_llm_message(msg: &LLMMessage) -> Vec<ContentBlock> {
    msg.content
        .iter()
        .filter_map(|content| from_llm_content(content))
        .collect()
}

/// Convert LLM client Content to our ContentBlock
pub fn from_llm_content(content: &LLMContent) -> Option<ContentBlock> {
    match content {
        LLMContent::Text(text) => Some(ContentBlock::Text(TextBlock { text: text.clone() })),
        LLMContent::ToolUse(tool_use) => {
            // Parse input JSON string to HashMap
            let input = serde_json::from_str(&tool_use.input).unwrap_or_default();
            Some(ContentBlock::ToolUse(ToolUseBlock {
                id: tool_use.id.clone(),
                name: tool_use.name.clone(),
                input,
            }))
        }
        LLMContent::ToolResult(tool_result) => Some(ContentBlock::ToolResult(ToolResultBlock {
            tool_use_id: tool_result.tool_use_id.clone(),
            content: tool_result.content.clone(),
            is_error: tool_result.is_error,
            compact_summary: None,
        })),
        LLMContent::Image(_) => None, // Skip images for now
    }
}

/// Convert a slice of our Messages to LLM client Messages
pub fn to_llm_messages(messages: &[Message]) -> Vec<LLMMessage> {
    messages.iter().map(to_llm_message).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::types::{TurnId, UserMessage};
    use std::collections::HashMap;

    #[test]
    fn test_text_content_conversion() {
        let block = ContentBlock::text("Hello world");
        let llm_msg = to_llm_content(&block).unwrap();

        if let LLMContent::Text(text) = llm_msg {
            assert_eq!(text, "Hello world");
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_user_message_conversion() {
        let msg = Message::User(UserMessage {
            id: "msg_1".to_string(),
            session_id: "sess_1".to_string(),
            turn_id: TurnId::new_user_turn(1),
            created_at: 0,
            content: vec![ContentBlock::text("Hello")],
        });

        let llm_msg = to_llm_message(&msg);
        assert_eq!(llm_msg.role, LLMRole::User);
        assert_eq!(llm_msg.content.len(), 1);
    }

    #[test]
    fn test_tool_use_roundtrip() {
        let mut input = HashMap::new();
        input.insert("query".to_string(), serde_json::json!("test"));

        let block = ContentBlock::ToolUse(ToolUseBlock {
            id: "tool_1".to_string(),
            name: "search".to_string(),
            input,
        });

        let llm_msg = to_llm_content(&block).unwrap();
        let back = from_llm_content(&llm_msg).unwrap();

        if let ContentBlock::ToolUse(tool) = back {
            assert_eq!(tool.id, "tool_1");
            assert_eq!(tool.name, "search");
        } else {
            panic!("Expected tool use");
        }
    }

    #[test]
    fn test_from_llm_text() {
        let content = LLMContent::Text("Response text".to_string());
        let block = from_llm_content(&content).unwrap();

        if let ContentBlock::Text(text) = block {
            assert_eq!(text.text, "Response text");
        } else {
            panic!("Expected text block");
        }
    }
}
