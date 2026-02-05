//! Bedrock Converse API request/response types and serialization.

use crate::client::error::LlmError;
use crate::client::models::{Content, Message, MessageOptions, Role, ToolChoice, ToolUse};
use crate::client::providers::common::escape_json_string;

// =============================================================================
// Constants
// =============================================================================

/// Bedrock runtime API base URL template.
const API_BASE_TEMPLATE: &str = "https://bedrock-runtime.{region}.amazonaws.com";

/// Converse API path template.
const CONVERSE_PATH: &str = "/model/{model}/converse";

/// Converse stream API path template.
const CONVERSE_STREAM_PATH: &str = "/model/{model}/converse-stream";

/// Error code for invalid request.
const ERROR_INVALID_REQUEST: &str = "INVALID_REQUEST";

/// Error code for parse errors.
const ERROR_PARSE: &str = "PARSE_ERROR";

/// Error message when no messages provided.
const MSG_NO_MESSAGES: &str = "At least one message is required";

/// Error message when no content in response.
const MSG_NO_CONTENT: &str = "No content found in response";

// =============================================================================
// Public API
// =============================================================================

/// Returns the Bedrock Converse API endpoint URL.
pub fn get_converse_url(region: &str, model: &str) -> String {
    let base = API_BASE_TEMPLATE.replace("{region}", region);
    let path = CONVERSE_PATH.replace("{model}", &url_encode(model));
    format!("{}{}", base, path)
}

/// Returns the Bedrock Converse Stream API endpoint URL.
pub fn get_converse_stream_url(region: &str, model: &str) -> String {
    let base = API_BASE_TEMPLATE.replace("{region}", region);
    let path = CONVERSE_STREAM_PATH.replace("{model}", &url_encode(model));
    format!("{}{}", base, path)
}

/// URL-encode a string for use in paths.
fn url_encode(s: &str) -> String {
    // Simple URL encoding for model IDs
    s.replace(':', "%3A")
}

/// Builds the JSON request body for the Bedrock Converse API.
///
/// Bedrock Converse format:
/// ```json
/// {
///   "messages": [{"role": "user", "content": [{"text": "..."}]}],
///   "system": [{"text": "..."}],
///   "inferenceConfig": {"maxTokens": 1024, "temperature": 0.7},
///   "toolConfig": {"tools": [...]}
/// }
/// ```
pub fn build_request_body(
    messages: &[Message],
    options: &MessageOptions,
) -> Result<String, LlmError> {
    // Separate system messages from conversation
    let mut system_texts: Vec<String> = Vec::new();
    let mut conversation_messages: Vec<&Message> = Vec::new();

    for msg in messages {
        match msg.role {
            Role::System => {
                for content in &msg.content {
                    if let Content::Text(text) = content {
                        system_texts.push(text.clone());
                    }
                }
            }
            Role::User | Role::Assistant => {
                conversation_messages.push(msg);
            }
        }
    }

    if conversation_messages.is_empty() {
        return Err(LlmError::new(ERROR_INVALID_REQUEST, MSG_NO_MESSAGES));
    }

    let mut json = String::with_capacity(2048);
    json.push('{');

    // Messages array
    json.push_str(r#""messages":["#);
    let mut first = true;
    for msg in &conversation_messages {
        if !first {
            json.push(',');
        }
        first = false;
        json.push_str(&format_message(msg)?);
    }
    json.push(']');

    // System instruction (optional)
    if !system_texts.is_empty() {
        json.push_str(r#","system":["#);
        for (i, text) in system_texts.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!(r#"{{"text":"{}"}}"#, escape_json_string(text)));
        }
        json.push(']');
    }

    // Inference config
    let mut inference_items: Vec<String> = Vec::new();
    if let Some(max_tokens) = options.max_tokens {
        inference_items.push(format!(r#""maxTokens":{}"#, max_tokens));
    }
    if let Some(temp) = options.temperature {
        inference_items.push(format!(r#""temperature":{}"#, temp));
    }
    if let Some(top_p) = options.top_p {
        inference_items.push(format!(r#""topP":{}"#, top_p));
    }
    if let Some(stop_sequences) = &options.stop_sequences {
        if !stop_sequences.is_empty() {
            let stops: Vec<String> = stop_sequences
                .iter()
                .map(|s| format!(r#""{}""#, escape_json_string(s)))
                .collect();
            inference_items.push(format!(r#""stopSequences":[{}]"#, stops.join(",")));
        }
    }

    if !inference_items.is_empty() {
        json.push_str(&format!(
            r#","inferenceConfig":{{{}}}"#,
            inference_items.join(",")
        ));
    }

    // Tool config (optional)
    if let Some(tools) = &options.tools {
        if !tools.is_empty() {
            json.push_str(r#","toolConfig":{"tools":["#);
            for (i, tool) in tools.iter().enumerate() {
                if i > 0 {
                    json.push(',');
                }
                // Bedrock tool format
                json.push_str(&format!(
                    r#"{{"toolSpec":{{"name":"{}","description":"{}","inputSchema":{{"json":{}}}}}}}"#,
                    escape_json_string(&tool.name),
                    escape_json_string(&tool.description),
                    tool.input_schema
                ));
            }
            json.push_str("]}");

            // Tool choice
            if let Some(tool_choice) = &options.tool_choice {
                match tool_choice {
                    ToolChoice::Auto => {
                        json.push_str(r#","toolChoice":{"auto":{}}"#);
                    }
                    ToolChoice::Any => {
                        json.push_str(r#","toolChoice":{"any":{}}"#);
                    }
                    ToolChoice::None => {
                        // Remove tool config if none
                    }
                    ToolChoice::Tool(name) => {
                        json.push_str(&format!(
                            r#","toolChoice":{{"tool":{{"name":"{}"}}}}"#,
                            escape_json_string(name)
                        ));
                    }
                }
            }
        }
    }

    json.push('}');

    Ok(json)
}

/// Formats a message for the Bedrock Converse API.
fn format_message(msg: &Message) -> Result<String, LlmError> {
    let role = match msg.role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => {
            return Err(LlmError::new(
                ERROR_INVALID_REQUEST,
                "System messages should be handled separately",
            ));
        }
    };

    let mut json = format!(r#"{{"role":"{}","content":["#, role);

    let mut first = true;
    for content in &msg.content {
        if !first {
            json.push(',');
        }
        first = false;

        match content {
            Content::Text(text) => {
                json.push_str(&format!(r#"{{"text":"{}"}}"#, escape_json_string(text)));
            }
            Content::Image(_) => {
                // Image support would require base64 encoding
                return Err(LlmError::new(
                    "IMAGE_NOT_SUPPORTED",
                    "Image content is not yet supported for Bedrock provider",
                ));
            }
            Content::ToolUse(tool_use) => {
                // Bedrock format for tool use in assistant messages
                json.push_str(&format!(
                    r#"{{"toolUse":{{"toolUseId":"{}","name":"{}","input":{}}}}}"#,
                    escape_json_string(&tool_use.id),
                    escape_json_string(&tool_use.name),
                    tool_use.input
                ));
            }
            Content::ToolResult(tool_result) => {
                // Bedrock format for tool results in user messages
                let status = if tool_result.is_error {
                    "error"
                } else {
                    "success"
                };
                json.push_str(&format!(
                    r#"{{"toolResult":{{"toolUseId":"{}","content":[{{"text":"{}"}}],"status":"{}"}}}}"#,
                    escape_json_string(&tool_result.tool_use_id),
                    escape_json_string(&tool_result.content),
                    status
                ));
            }
        }
    }

    json.push_str("]}");
    Ok(json)
}

/// Parses the Bedrock Converse API response and extracts the assistant message.
pub fn parse_response(response_body: &str) -> Result<Message, LlmError> {
    let parsed: serde_json::Value = serde_json::from_str(response_body)
        .map_err(|e| LlmError::new(ERROR_PARSE, format!("Failed to parse response: {}", e)))?;

    // Check for API error response
    if let Some(error_msg) = parsed["message"].as_str() {
        if parsed.get("output").is_none() {
            let error_type = parsed["__type"].as_str().unwrap_or("BedrockError");
            return Err(LlmError::new(error_type, error_msg));
        }
    }

    // Extract output message
    let output = &parsed["output"]["message"];
    if output.is_null() {
        return Err(LlmError::new(ERROR_PARSE, MSG_NO_CONTENT));
    }

    let mut content_blocks: Vec<Content> = Vec::new();

    // Parse content array
    if let Some(content_arr) = output["content"].as_array() {
        for content in content_arr {
            // Text content
            if let Some(text) = content["text"].as_str() {
                if !text.is_empty() {
                    content_blocks.push(Content::Text(text.to_string()));
                }
            }
            // Tool use
            if let Some(tool_use) = content.get("toolUse") {
                let id = tool_use["toolUseId"].as_str().unwrap_or("").to_string();
                let name = tool_use["name"].as_str().unwrap_or("").to_string();
                let input = tool_use["input"].to_string();

                content_blocks.push(Content::ToolUse(ToolUse { id, name, input }));
            }
        }
    }

    if content_blocks.is_empty() {
        return Err(LlmError::new(ERROR_PARSE, MSG_NO_CONTENT));
    }

    Ok(Message::with_content(Role::Assistant, content_blocks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::models::Tool;

    #[test]
    fn test_get_converse_url() {
        let url = get_converse_url("us-east-1", "anthropic.claude-3-sonnet-20240229-v1:0");
        assert_eq!(
            url,
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/anthropic.claude-3-sonnet-20240229-v1%3A0/converse"
        );
    }

    #[test]
    fn test_get_converse_stream_url() {
        let url = get_converse_stream_url("us-west-2", "anthropic.claude-3-haiku-20240307-v1:0");
        assert_eq!(
            url,
            "https://bedrock-runtime.us-west-2.amazonaws.com/model/anthropic.claude-3-haiku-20240307-v1%3A0/converse-stream"
        );
    }

    #[test]
    fn test_build_request_body_simple() {
        let messages = vec![Message::user("Hello")];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["messages"].is_array());
        assert_eq!(parsed["messages"][0]["role"], "user");
        assert_eq!(parsed["messages"][0]["content"][0]["text"], "Hello");
    }

    #[test]
    fn test_build_request_body_with_system() {
        let messages = vec![Message::system("You are helpful"), Message::user("Hello")];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["system"].is_array());
        assert_eq!(parsed["system"][0]["text"], "You are helpful");
        assert_eq!(parsed["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_build_request_body_with_options() {
        let messages = vec![Message::user("Hello")];
        let options = MessageOptions {
            max_tokens: Some(1024),
            temperature: Some(0.7),
            top_p: Some(0.9),
            ..Default::default()
        };

        let json = build_request_body(&messages, &options).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["inferenceConfig"]["maxTokens"], 1024);
        assert_eq!(parsed["inferenceConfig"]["temperature"], 0.7);
        assert_eq!(parsed["inferenceConfig"]["topP"], 0.9);
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let messages = vec![Message::user("What's the weather?")];
        let options = MessageOptions {
            tools: Some(vec![Tool::new(
                "get_weather",
                "Get weather for a location",
                r#"{"type":"object","properties":{"location":{"type":"string"}}}"#,
            )]),
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        };

        let json = build_request_body(&messages, &options).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["toolConfig"]["tools"].is_array());
        assert_eq!(
            parsed["toolConfig"]["tools"][0]["toolSpec"]["name"],
            "get_weather"
        );
    }

    #[test]
    fn test_parse_response_text() {
        let response = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{"text": "Hello! How can I help?"}]
                }
            },
            "stopReason": "end_turn"
        }"#;

        let msg = parse_response(response).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        match &msg.content[0] {
            Content::Text(t) => assert_eq!(t, "Hello! How can I help?"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_parse_response_tool_use() {
        let response = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [
                        {"toolUse": {"toolUseId": "call_123", "name": "get_weather", "input": {"location": "SF"}}}
                    ]
                }
            },
            "stopReason": "tool_use"
        }"#;

        let msg = parse_response(response).unwrap();
        assert_eq!(msg.content.len(), 1);
        match &msg.content[0] {
            Content::ToolUse(tu) => {
                assert_eq!(tu.id, "call_123");
                assert_eq!(tu.name, "get_weather");
            }
            _ => panic!("Expected tool use"),
        }
    }

    #[test]
    fn test_parse_response_error() {
        let response = r#"{
            "__type": "ValidationException",
            "message": "Invalid model ID"
        }"#;

        let err = parse_response(response).unwrap_err();
        assert_eq!(err.error_code, "ValidationException");
    }
}
