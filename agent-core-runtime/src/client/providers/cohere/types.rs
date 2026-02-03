//! Cohere API request/response types and serialization.

use crate::client::error::LlmError;
use crate::client::models::{Content, Message, MessageOptions, Role, ToolChoice, ToolUse};
use crate::client::providers::common::escape_json_string;

// =============================================================================
// Constants
// =============================================================================

/// Cohere chat API endpoint (v2).
const API_URL: &str = "https://api.cohere.com/v2/chat";

/// HTTP header for content type.
const HEADER_CONTENT_TYPE: &str = "Content-Type";

/// HTTP header for authorization.
const HEADER_AUTHORIZATION: &str = "Authorization";

/// Content type for JSON requests.
const CONTENT_TYPE_JSON: &str = "application/json";

/// Error code for invalid request.
const ERROR_INVALID_REQUEST: &str = "INVALID_REQUEST";

/// Error code for parse errors.
const ERROR_PARSE: &str = "PARSE_ERROR";

/// Error message when API key is empty.
const MSG_EMPTY_API_KEY: &str = "API key cannot be empty";

/// Error message when no messages provided.
const MSG_NO_MESSAGES: &str = "At least one message is required";

/// Error message when no content in response.
const MSG_NO_CONTENT: &str = "No content found in response";

// =============================================================================
// Public API
// =============================================================================

/// Returns the Cohere chat API endpoint URL.
pub fn get_api_url() -> String {
    API_URL.to_string()
}

/// Returns the HTTP headers required for the Cohere API.
///
/// # Errors
/// Returns an error if the API key is empty.
pub fn get_request_headers(api_key: &str) -> Result<Vec<(&'static str, String)>, LlmError> {
    if api_key.is_empty() {
        return Err(LlmError::new(ERROR_INVALID_REQUEST, MSG_EMPTY_API_KEY));
    }
    Ok(vec![
        (HEADER_CONTENT_TYPE, CONTENT_TYPE_JSON.to_string()),
        (HEADER_AUTHORIZATION, format!("Bearer {}", api_key)),
    ])
}

/// Builds the JSON request body for the Cohere Chat API.
pub fn build_request_body(
    messages: &[Message],
    options: &MessageOptions,
    default_model: &str,
) -> Result<String, LlmError> {
    build_request_body_internal(messages, options, default_model, false)
}

/// Builds the JSON request body for streaming Cohere Chat API.
pub fn build_streaming_request_body(
    messages: &[Message],
    options: &MessageOptions,
    default_model: &str,
) -> Result<String, LlmError> {
    build_request_body_internal(messages, options, default_model, true)
}

fn build_request_body_internal(
    messages: &[Message],
    options: &MessageOptions,
    default_model: &str,
    stream: bool,
) -> Result<String, LlmError> {
    if messages.is_empty() {
        return Err(LlmError::new(ERROR_INVALID_REQUEST, MSG_NO_MESSAGES));
    }

    let model = options.model.as_deref().unwrap_or(default_model);

    let mut json = String::with_capacity(2048);
    json.push('{');

    // Model
    json.push_str(&format!(r#""model":"{}""#, escape_json_string(model)));

    // Stream flag
    if stream {
        json.push_str(r#","stream":true"#);
    }

    // Messages array
    json.push_str(r#","messages":["#);
    let mut first = true;
    for msg in messages {
        if !first {
            json.push(',');
        }
        first = false;
        json.push_str(&format_message(msg)?);
    }
    json.push(']');

    // Max tokens (optional)
    if let Some(max_tokens) = options.max_tokens {
        json.push_str(&format!(r#","max_tokens":{}"#, max_tokens));
    }

    // Temperature (optional)
    if let Some(temp) = options.temperature {
        json.push_str(&format!(r#","temperature":{}"#, temp));
    }

    // Top P (optional)
    if let Some(top_p) = options.top_p {
        json.push_str(&format!(r#","p":{}"#, top_p));
    }

    // Top K (optional)
    if let Some(top_k) = options.top_k {
        json.push_str(&format!(r#","k":{}"#, top_k));
    }

    // Stop sequences (optional)
    if let Some(stop_sequences) = &options.stop_sequences {
        if !stop_sequences.is_empty() {
            json.push_str(r#","stop_sequences":["#);
            for (i, seq) in stop_sequences.iter().enumerate() {
                if i > 0 {
                    json.push(',');
                }
                json.push_str(&format!(r#""{}""#, escape_json_string(seq)));
            }
            json.push(']');
        }
    }

    // Tools (optional) - Cohere format
    if let Some(tools) = &options.tools {
        if !tools.is_empty() {
            json.push_str(r#","tools":["#);
            for (i, tool) in tools.iter().enumerate() {
                if i > 0 {
                    json.push(',');
                }
                // Cohere uses "parameter_definitions" directly, not wrapped in "function"
                json.push_str(&format!(
                    r#"{{"type":"function","function":{{"name":"{}","description":"{}","parameters":{}}}}}"#,
                    escape_json_string(&tool.name),
                    escape_json_string(&tool.description),
                    tool.input_schema
                ));
            }
            json.push(']');
        }
    }

    // Tool choice (optional) - Cohere uses "tool_choice" with object format
    if let Some(tool_choice) = &options.tool_choice {
        match tool_choice {
            ToolChoice::Auto => {
                // Cohere default is auto, no need to specify
            }
            ToolChoice::Any => {
                json.push_str(r#","tool_choice":"required""#);
            }
            ToolChoice::None => {
                json.push_str(r#","tool_choice":"none""#);
            }
            ToolChoice::Tool(name) => {
                json.push_str(&format!(
                    r#","tool_choice":{{"type":"function","function":{{"name":"{}"}}}}"#,
                    escape_json_string(name)
                ));
            }
        }
    }

    json.push('}');

    Ok(json)
}

/// Formats a message for the Cohere API.
fn format_message(msg: &Message) -> Result<String, LlmError> {
    let role = match msg.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    };

    // Check if message has tool calls (assistant) or tool results (user)
    let has_tool_calls = msg
        .content
        .iter()
        .any(|c| matches!(c, Content::ToolUse(_)));
    let has_tool_results = msg
        .content
        .iter()
        .any(|c| matches!(c, Content::ToolResult(_)));

    if has_tool_results {
        // Tool results in Cohere are sent as role "tool"
        if let Some(Content::ToolResult(tr)) = msg.content.iter().find(|c| matches!(c, Content::ToolResult(_))) {
            return Ok(format!(
                r#"{{"role":"tool","tool_call_id":"{}","content":"{}"}}"#,
                escape_json_string(&tr.tool_use_id),
                escape_json_string(&tr.content)
            ));
        }
    }

    if has_tool_calls {
        // Assistant message with tool calls
        let mut json = format!(r#"{{"role":"{}""#, role);

        // Add text content if present
        let text_content: Vec<&str> = msg
            .content
            .iter()
            .filter_map(|c| match c {
                Content::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();

        if !text_content.is_empty() {
            json.push_str(&format!(
                r#","content":"{}""#,
                escape_json_string(&text_content.join("\n"))
            ));
        }

        // Add tool_calls array
        json.push_str(r#","tool_calls":["#);
        let mut first = true;
        for content in &msg.content {
            if let Content::ToolUse(tu) = content {
                if !first {
                    json.push(',');
                }
                first = false;
                json.push_str(&format!(
                    r#"{{"id":"{}","type":"function","function":{{"name":"{}","arguments":{}}}}}"#,
                    escape_json_string(&tu.id),
                    escape_json_string(&tu.name),
                    tu.input
                ));
            }
        }
        json.push_str("]}");
        return Ok(json);
    }

    // Simple text message
    let text_content: Vec<&str> = msg
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();

    Ok(format!(
        r#"{{"role":"{}","content":"{}"}}"#,
        role,
        escape_json_string(&text_content.join("\n"))
    ))
}

/// Parses the Cohere API response and extracts the assistant message.
pub fn parse_response(response_body: &str) -> Result<Message, LlmError> {
    let parsed: serde_json::Value = serde_json::from_str(response_body)
        .map_err(|e| LlmError::new(ERROR_PARSE, format!("Failed to parse response: {}", e)))?;

    // Check for API error response
    if let Some(error_msg) = parsed["message"].as_str() {
        // Cohere returns errors with "message" field
        if parsed.get("text").is_none() && parsed.get("tool_calls").is_none() {
            return Err(LlmError::new("COHERE_ERROR", error_msg));
        }
    }

    let mut content_blocks: Vec<Content> = Vec::new();

    // Extract text content from "message.content[].text"
    if let Some(message) = parsed.get("message") {
        if let Some(content_arr) = message["content"].as_array() {
            for content in content_arr {
                if let Some(text) = content["text"].as_str() {
                    if !text.is_empty() {
                        content_blocks.push(Content::Text(text.to_string()));
                    }
                }
            }
        }

        // Extract tool calls from "message.tool_calls"
        if let Some(tool_calls) = message["tool_calls"].as_array() {
            for tc in tool_calls {
                let id = tc["id"].as_str().unwrap_or("").to_string();
                let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                let arguments = tc["function"]["arguments"]
                    .as_str()
                    .unwrap_or("{}")
                    .to_string();

                content_blocks.push(Content::ToolUse(ToolUse {
                    id,
                    name,
                    input: arguments,
                }));
            }
        }
    }

    // Fallback: check for top-level "text" field (older API format)
    if content_blocks.is_empty() {
        if let Some(text) = parsed["text"].as_str() {
            if !text.is_empty() {
                content_blocks.push(Content::Text(text.to_string()));
            }
        }

        // Check for top-level tool_calls
        if let Some(tool_calls) = parsed["tool_calls"].as_array() {
            for tc in tool_calls {
                let id = tc["id"].as_str().unwrap_or("").to_string();
                let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                let arguments = tc["function"]["arguments"]
                    .as_str()
                    .unwrap_or("{}")
                    .to_string();

                content_blocks.push(Content::ToolUse(ToolUse {
                    id,
                    name,
                    input: arguments,
                }));
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
    fn test_get_api_url() {
        assert_eq!(get_api_url(), "https://api.cohere.com/v2/chat");
    }

    #[test]
    fn test_get_request_headers_valid() {
        let headers = get_request_headers("test-api-key").unwrap();
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0], (HEADER_CONTENT_TYPE, CONTENT_TYPE_JSON.to_string()));
        assert_eq!(headers[1], (HEADER_AUTHORIZATION, "Bearer test-api-key".to_string()));
    }

    #[test]
    fn test_get_request_headers_empty_key() {
        let result = get_request_headers("");
        assert!(result.is_err());
    }

    #[test]
    fn test_build_request_body_simple() {
        let messages = vec![Message::user("Hello")];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options, "command-r-plus").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["model"], "command-r-plus");
        assert!(parsed["messages"].is_array());
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
            ..Default::default()
        };

        let json = build_request_body(&messages, &options, "command-r-plus").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["tools"].is_array());
        assert_eq!(parsed["tools"][0]["function"]["name"], "get_weather");
    }

    #[test]
    fn test_build_streaming_request_body() {
        let messages = vec![Message::user("Hello")];
        let options = MessageOptions::default();

        let json = build_streaming_request_body(&messages, &options, "command-r-plus").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["stream"], true);
    }

    #[test]
    fn test_parse_response_text() {
        let response = r#"{
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Hello! How can I help?"}]
            }
        }"#;

        let msg = parse_response(response).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        match &msg.content[0] {
            Content::Text(t) => assert_eq!(t, "Hello! How can I help?"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_parse_response_tool_calls() {
        let response = r#"{
            "message": {
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_123",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"location\":\"SF\"}"
                    }
                }]
            }
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
}
