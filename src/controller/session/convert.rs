// Type conversion between llm-controller-rs and vangogh-rs types

use vangogh_rs::models::{
    Content as VangoghContent, Message as VangoghMessage, Role as VangoghRole,
    ToolResult as VangoghToolResult, ToolUse as VangoghToolUse,
};

use crate::types::{ContentBlock, Message, MessageRole, TextBlock, ToolResultBlock, ToolUseBlock};

/// Convert our Message to vangogh-rs Message
pub fn to_vangogh_message(msg: &Message) -> VangoghMessage {
    let role = match msg.role() {
        MessageRole::User => VangoghRole::User,
        MessageRole::Assistant => VangoghRole::Assistant,
    };

    let content: Vec<VangoghContent> = msg
        .content()
        .iter()
        .filter_map(|block| to_vangogh_content(block))
        .collect();

    VangoghMessage::with_content(role, content)
}

/// Convert our ContentBlock to vangogh-rs Content
pub fn to_vangogh_content(block: &ContentBlock) -> Option<VangoghContent> {
    match block {
        ContentBlock::Text(text_block) => Some(VangoghContent::Text(text_block.text.clone())),
        ContentBlock::ToolUse(tool_use) => {
            Some(VangoghContent::ToolUse(VangoghToolUse {
                id: tool_use.id.clone(),
                name: tool_use.name.clone(),
                // Convert HashMap<String, Value> to JSON string
                input: serde_json::to_string(&tool_use.input).unwrap_or_default(),
            }))
        }
        ContentBlock::ToolResult(tool_result) => {
            Some(VangoghContent::ToolResult(VangoghToolResult {
                tool_use_id: tool_result.tool_use_id.clone(),
                content: tool_result.content.clone(),
                is_error: tool_result.is_error,
            }))
        }
    }
}

/// Convert vangogh-rs Message to our ContentBlock list
pub fn from_vangogh_message(msg: &VangoghMessage) -> Vec<ContentBlock> {
    msg.content
        .iter()
        .filter_map(|content| from_vangogh_content(content))
        .collect()
}

/// Convert vangogh-rs Content to our ContentBlock
pub fn from_vangogh_content(content: &VangoghContent) -> Option<ContentBlock> {
    match content {
        VangoghContent::Text(text) => Some(ContentBlock::Text(TextBlock { text: text.clone() })),
        VangoghContent::ToolUse(tool_use) => {
            // Parse input JSON string to HashMap
            let input = serde_json::from_str(&tool_use.input).unwrap_or_default();
            Some(ContentBlock::ToolUse(ToolUseBlock {
                id: tool_use.id.clone(),
                name: tool_use.name.clone(),
                input,
            }))
        }
        VangoghContent::ToolResult(tool_result) => Some(ContentBlock::ToolResult(ToolResultBlock {
            tool_use_id: tool_result.tool_use_id.clone(),
            content: tool_result.content.clone(),
            is_error: tool_result.is_error,
            compact_summary: None,
        })),
        VangoghContent::Image(_) => None, // Skip images for now
    }
}

/// Convert a slice of our Messages to vangogh-rs Messages
pub fn to_vangogh_messages(messages: &[Message]) -> Vec<VangoghMessage> {
    messages.iter().map(to_vangogh_message).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TurnId, UserMessage};
    use std::collections::HashMap;

    #[test]
    fn test_text_content_conversion() {
        let block = ContentBlock::text("Hello world");
        let vangogh = to_vangogh_content(&block).unwrap();

        if let VangoghContent::Text(text) = vangogh {
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

        let vangogh = to_vangogh_message(&msg);
        assert_eq!(vangogh.role, VangoghRole::User);
        assert_eq!(vangogh.content.len(), 1);
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

        let vangogh = to_vangogh_content(&block).unwrap();
        let back = from_vangogh_content(&vangogh).unwrap();

        if let ContentBlock::ToolUse(tool) = back {
            assert_eq!(tool.id, "tool_1");
            assert_eq!(tool.name, "search");
        } else {
            panic!("Expected tool use");
        }
    }

    #[test]
    fn test_from_vangogh_text() {
        let content = VangoghContent::Text("Response text".to_string());
        let block = from_vangogh_content(&content).unwrap();

        if let ContentBlock::Text(text) = block {
            assert_eq!(text.text, "Response text");
        } else {
            panic!("Expected text block");
        }
    }
}
