use crate::client::error::LlmError;
use crate::client::models::{
    Content, ImageSource, Message, MessageOptions, Role, ToolChoice, ToolUse,
};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Builds a streaming JSON request body for the Anthropic Messages API.
pub fn build_streaming_request_body(
    messages: &[Message],
    options: &MessageOptions,
    default_model: &str,
) -> Result<String, LlmError> {
    let mut body = build_request_body(messages, options, default_model)?;
    // Insert stream:true before the closing brace
    body.pop(); // Remove trailing }
    body.push_str(r#","stream":true}"#);
    Ok(body)
}

/// Builds the JSON request body for the Anthropic Messages API.
/// Anthropic requires system messages in a separate "system" field.
pub fn build_request_body(
    messages: &[Message],
    options: &MessageOptions,
    default_model: &str,
) -> Result<String, LlmError> {
    let mut system_prompt: Option<String> = None;
    let mut conversation_messages: Vec<&Message> = Vec::new();

    // Separate system messages from conversation messages
    for msg in messages {
        match msg.role {
            Role::System => {
                // Combine multiple system messages if present
                let text = extract_text_content(msg);
                if let Some(existing) = system_prompt.take() {
                    system_prompt = Some(format!("{}\n{}", existing, text));
                } else {
                    system_prompt = Some(text);
                }
            }
            Role::User | Role::Assistant => {
                conversation_messages.push(msg);
            }
        }
    }

    if conversation_messages.is_empty() {
        return Err(LlmError::new(
            "INVALID_REQUEST",
            "At least one user or assistant message is required",
        ));
    }

    let model = options.model.as_deref().unwrap_or(default_model);
    let max_tokens = options.max_tokens.unwrap_or(1024);

    let mut json = String::with_capacity(2048);
    json.push('{');

    // Model
    json.push_str(&format!(r#""model":"{}""#, escape_json_string(model)));

    // Max tokens
    json.push_str(&format!(r#","max_tokens":{}"#, max_tokens));

    // Temperature (optional)
    if let Some(temp) = options.temperature {
        json.push_str(&format!(r#","temperature":{}"#, temp));
    }

    // Top P (optional)
    if let Some(top_p) = options.top_p {
        json.push_str(&format!(r#","top_p":{}"#, top_p));
    }

    // Top K (optional)
    if let Some(top_k) = options.top_k {
        json.push_str(&format!(r#","top_k":{}"#, top_k));
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

    // System prompt (optional)
    if let Some(system) = &system_prompt {
        json.push_str(&format!(r#","system":"{}""#, escape_json_string(system)));
    }

    // Tools (optional)
    if let Some(tools) = &options.tools {
        if !tools.is_empty() {
            json.push_str(r#","tools":["#);
            for (i, tool) in tools.iter().enumerate() {
                if i > 0 {
                    json.push(',');
                }
                json.push_str(&format!(
                    r#"{{"name":"{}","description":"{}","input_schema":{}}}"#,
                    escape_json_string(&tool.name),
                    escape_json_string(&tool.description),
                    tool.input_schema // Already JSON
                ));
            }
            json.push(']');
        }
    }

    // Tool choice (optional)
    if let Some(tool_choice) = &options.tool_choice {
        match tool_choice {
            ToolChoice::Auto => {
                json.push_str(r#","tool_choice":{"type":"auto"}"#);
            }
            ToolChoice::Any => {
                json.push_str(r#","tool_choice":{"type":"any"}"#);
            }
            ToolChoice::None => {
                json.push_str(r#","tool_choice":{"type":"none"}"#);
            }
            ToolChoice::Tool(name) => {
                json.push_str(&format!(
                    r#","tool_choice":{{"type":"tool","name":"{}"}}"#,
                    escape_json_string(name)
                ));
            }
        }
    }

    // Metadata (optional)
    if let Some(metadata) = &options.metadata {
        if let Some(user_id) = &metadata.user_id {
            json.push_str(&format!(
                r#","metadata":{{"user_id":"{}"}}"#,
                escape_json_string(user_id)
            ));
        }
    }

    // Messages array
    json.push_str(r#","messages":["#);
    for (i, msg) in conversation_messages.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format_message(msg));
    }
    json.push(']');

    json.push('}');

    Ok(json)
}

/// Returns the HTTP headers required for the Anthropic API.
pub fn get_request_headers(api_key: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Content-Type", "application/json".to_string()),
        ("x-api-key", api_key.to_string()),
        ("anthropic-version", ANTHROPIC_VERSION.to_string()),
    ]
}

/// Returns the Anthropic API endpoint URL.
pub fn get_api_url() -> &'static str {
    ANTHROPIC_API_URL
}

fn format_message(msg: &Message) -> String {
    let role = match msg.role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "user", // Should not happen, but fallback
    };

    let mut json = format!(r#"{{"role":"{}","content":["#, role);

    for (i, content) in msg.content.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format_content_block(content));
    }

    json.push_str("]}");
    json
}

fn format_content_block(content: &Content) -> String {
    match content {
        Content::Text(text) => {
            format!(r#"{{"type":"text","text":"{}"}}"#, escape_json_string(text))
        }
        Content::Image(source) => match source {
            ImageSource::Base64 { media_type, data } => {
                format!(
                    r#"{{"type":"image","source":{{"type":"base64","media_type":"{}","data":"{}"}}}}"#,
                    escape_json_string(media_type),
                    escape_json_string(data)
                )
            }
            ImageSource::Url(url) => {
                format!(
                    r#"{{"type":"image","source":{{"type":"url","url":"{}"}}}}"#,
                    escape_json_string(url)
                )
            }
        },
        Content::ToolUse(tool_use) => {
            format!(
                r#"{{"type":"tool_use","id":"{}","name":"{}","input":{}}}"#,
                escape_json_string(&tool_use.id),
                escape_json_string(&tool_use.name),
                tool_use.input // Already JSON
            )
        }
        Content::ToolResult(tool_result) => {
            if tool_result.is_error {
                format!(
                    r#"{{"type":"tool_result","tool_use_id":"{}","content":"{}","is_error":true}}"#,
                    escape_json_string(&tool_result.tool_use_id),
                    escape_json_string(&tool_result.content)
                )
            } else {
                format!(
                    r#"{{"type":"tool_result","tool_use_id":"{}","content":"{}"}}"#,
                    escape_json_string(&tool_result.tool_use_id),
                    escape_json_string(&tool_result.content)
                )
            }
        }
    }
}

fn extract_text_content(msg: &Message) -> String {
    msg.content
        .iter()
        .filter_map(|c| match c {
            Content::Text(t) => Some(t.as_str()),
            Content::Image(_) | Content::ToolUse(_) | Content::ToolResult(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Escapes special characters for JSON string values.
fn escape_json_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str(r#"\""#),
            '\\' => result.push_str(r#"\\"#),
            '\n' => result.push_str(r#"\n"#),
            '\r' => result.push_str(r#"\r"#),
            '\t' => result.push_str(r#"\t"#),
            c if c.is_control() => {
                result.push_str(&format!(r#"\u{:04x}"#, c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

/// Parses the Anthropic API response and extracts the assistant message.
pub fn parse_response(response_body: &str) -> Result<Message, LlmError> {
    let parsed: serde_json::Value = serde_json::from_str(response_body)
        .map_err(|e| LlmError::new("PARSE_ERROR", format!("Failed to parse response: {}", e)))?;

    // Check for API error response
    if let Some(error_type) = parsed["error"]["type"].as_str() {
        let error_msg = parsed["error"]["message"]
            .as_str()
            .unwrap_or("Unknown error");
        return Err(LlmError::new(error_type, error_msg));
    }

    // Extract content from successful response
    // Anthropic returns: { "content": [{"type": "text", "text": "..."}, {"type": "tool_use", ...}], ... }
    let content_array = &parsed["content"];

    let mut content_blocks: Vec<Content> = Vec::new();

    // Use as_array() iterator for arrays
    if let Some(elements) = content_array.as_array() {
        for block in elements {
            if let Some(block_type) = block["type"].as_str() {
                match block_type {
                    "text" => {
                        // serde_json properly unescapes JSON strings
                        if let Some(text) = block["text"].as_str() {
                            content_blocks.push(Content::Text(text.to_string()));
                        }
                    }
                    "tool_use" => {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        // Get the input as raw JSON string
                        let input = block["input"].to_string();

                        content_blocks.push(Content::ToolUse(ToolUse { id, name, input }));
                    }
                    _ => {
                        // Skip unknown block types
                    }
                }
            }
        }
    }

    if content_blocks.is_empty() {
        return Err(LlmError::new("PARSE_ERROR", "No content found in response"));
    }

    Ok(Message::with_content(Role::Assistant, content_blocks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::models::Tool;

    #[test]
    fn test_build_request_body_simple() {
        let messages = vec![Message::user("Hello")];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options, "claude-3-sonnet-20240229").unwrap();

        assert!(json.contains(r#""model":"claude-3-sonnet-20240229""#));
        assert!(json.contains(r#""max_tokens":1024"#));
        assert!(json.contains(r#""role":"user""#));
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains(r#""text":"Hello""#));
    }

    #[test]
    fn test_build_request_body_with_system() {
        let messages = vec![Message::system("You are helpful"), Message::user("Hello")];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options, "claude-3-sonnet-20240229").unwrap();

        assert!(json.contains(r#""system":"You are helpful""#));
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let messages = vec![Message::user("What's the weather?")];
        let options = MessageOptions {
            tools: Some(vec![Tool::new(
                "get_weather",
                "Get weather for a location",
                r#"{"type":"object","properties":{"location":{"type":"string"}},"required":["location"]}"#,
            )]),
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        };

        let json = build_request_body(&messages, &options, "claude-3-sonnet-20240229").unwrap();

        assert!(json.contains(r#""tools":["#));
        assert!(json.contains(r#""name":"get_weather""#));
        assert!(json.contains(r#""tool_choice":{"type":"auto"}"#));
    }

    #[test]
    fn test_build_request_body_with_tool_result() {
        let messages = vec![
            Message::user("What's the weather in SF?"),
            Message::with_content(
                Role::Assistant,
                vec![Content::ToolUse(ToolUse {
                    id: "toolu_123".to_string(),
                    name: "get_weather".to_string(),
                    input: r#"{"location":"San Francisco"}"#.to_string(),
                })],
            ),
            Message::tool_result("toolu_123", "72F and sunny", false),
        ];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options, "claude-3-sonnet-20240229").unwrap();

        assert!(json.contains(r#""type":"tool_use""#));
        assert!(json.contains(r#""id":"toolu_123""#));
        assert!(json.contains(r#""type":"tool_result""#));
        assert!(json.contains(r#""tool_use_id":"toolu_123""#));
    }

    #[test]
    fn test_escape_json_string() {
        assert_eq!(escape_json_string(r#"hello"world"#), r#"hello\"world"#);
        assert_eq!(escape_json_string("line1\nline2"), r#"line1\nline2"#);
    }

    #[test]
    fn test_parse_response_text() {
        let response = r#"{
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello! How can I help you?"}],
            "model": "claude-3-sonnet-20240229",
            "stop_reason": "end_turn"
        }"#;

        let msg = parse_response(response).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        match &msg.content[0] {
            Content::Text(t) => assert_eq!(t, "Hello! How can I help you?"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_parse_response_tool_use() {
        let response = r#"{
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Let me check the weather."},
                {"type": "tool_use", "id": "toolu_456", "name": "get_weather", "input": {"location": "SF"}}
            ],
            "model": "claude-3-sonnet-20240229",
            "stop_reason": "tool_use"
        }"#;

        let msg = parse_response(response).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.content.len(), 2);

        match &msg.content[0] {
            Content::Text(t) => assert_eq!(t, "Let me check the weather."),
            _ => panic!("Expected text content"),
        }

        match &msg.content[1] {
            Content::ToolUse(tu) => {
                assert_eq!(tu.id, "toolu_456");
                assert_eq!(tu.name, "get_weather");
            }
            _ => panic!("Expected tool use content"),
        }
    }

    #[test]
    fn test_parse_response_error() {
        let response = r#"{
            "type": "error",
            "error": {
                "type": "invalid_api_key",
                "message": "Invalid API key provided"
            }
        }"#;

        let err = parse_response(response).unwrap_err();
        assert_eq!(err.error_code, "invalid_api_key");
    }
}
