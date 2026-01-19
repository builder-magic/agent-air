use crate::error::LlmError;
use crate::models::{Content, ImageSource, Message, MessageOptions, Role, ToolChoice, ToolUse};

const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

/// Builds the JSON request body for the OpenAI Chat API.
pub fn build_request_body(
    messages: &[Message],
    options: &MessageOptions,
    default_model: &str,
) -> Result<String, LlmError> {
    if messages.is_empty() {
        return Err(LlmError::new(
            "INVALID_REQUEST",
            "At least one message is required",
        ));
    }

    let model = options.model.as_deref().unwrap_or(default_model);

    let mut json = String::with_capacity(2048);
    json.push('{');

    // Model
    json.push_str(&format!(r#""model":"{}""#, escape_json_string(model)));

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
        json.push_str(&format!(r#","top_p":{}"#, top_p));
    }

    // Stop sequences (optional) - OpenAI uses "stop" not "stop_sequences"
    if let Some(stop_sequences) = &options.stop_sequences {
        if !stop_sequences.is_empty() {
            json.push_str(r#","stop":["#);
            for (i, seq) in stop_sequences.iter().enumerate() {
                if i > 0 {
                    json.push(',');
                }
                json.push_str(&format!(r#""{}""#, escape_json_string(seq)));
            }
            json.push(']');
        }
    }

    // Tools (optional) - OpenAI wraps each tool in a "function" type
    if let Some(tools) = &options.tools {
        if !tools.is_empty() {
            json.push_str(r#","tools":["#);
            for (i, tool) in tools.iter().enumerate() {
                if i > 0 {
                    json.push(',');
                }
                json.push_str(&format!(
                    r#"{{"type":"function","function":{{"name":"{}","description":"{}","parameters":{}}}}}"#,
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
                json.push_str(r#","tool_choice":"auto""#);
            }
            ToolChoice::Any => {
                // OpenAI uses "required" instead of "any"
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

    // User ID (optional) - OpenAI puts this at top level
    if let Some(metadata) = &options.metadata {
        if let Some(user_id) = &metadata.user_id {
            json.push_str(&format!(r#","user":"{}""#, escape_json_string(user_id)));
        }
    }

    // Messages array
    json.push_str(r#","messages":["#);
    for (i, msg) in messages.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format_message(msg)?);
    }
    json.push(']');

    json.push('}');

    Ok(json)
}

/// Returns the HTTP headers required for the OpenAI API.
pub fn get_request_headers(api_key: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Content-Type", "application/json".to_string()),
        ("Authorization", format!("Bearer {}", api_key)),
    ]
}

/// Returns the OpenAI API endpoint URL.
pub fn get_api_url() -> &'static str {
    OPENAI_API_URL
}

fn format_message(msg: &Message) -> Result<String, LlmError> {
    let role = match msg.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    };

    // Check if message has only simple text content
    let is_simple_text = msg.content.len() == 1 && matches!(&msg.content[0], Content::Text(_));

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
        // Tool results in OpenAI are sent as separate messages with role "tool"
        // For simplicity, we'll send the first tool result
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
        } else {
            json.push_str(r#","content":null"#);
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
                    tu.input // Already JSON
                ));
            }
        }
        json.push_str("]}");
        return Ok(json);
    }

    if is_simple_text {
        // Simple text message - content is a string
        if let Content::Text(text) = &msg.content[0] {
            return Ok(format!(
                r#"{{"role":"{}","content":"{}"}}"#,
                role,
                escape_json_string(text)
            ));
        }
    }

    // Complex content (images, multiple parts) - content is an array
    let mut json = format!(r#"{{"role":"{}","content":["#, role);

    for (i, content) in msg.content.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format_content_block(content));
    }

    json.push_str("]}");
    Ok(json)
}

fn format_content_block(content: &Content) -> String {
    match content {
        Content::Text(text) => {
            format!(r#"{{"type":"text","text":"{}"}}"#, escape_json_string(text))
        }
        Content::Image(source) => match source {
            ImageSource::Base64 { media_type, data } => {
                format!(
                    r#"{{"type":"image_url","image_url":{{"url":"data:{};base64,{}"}}}}"#,
                    escape_json_string(media_type),
                    escape_json_string(data)
                )
            }
            ImageSource::Url(url) => {
                format!(
                    r#"{{"type":"image_url","image_url":{{"url":"{}"}}}}"#,
                    escape_json_string(url)
                )
            }
        },
        Content::ToolUse(_) | Content::ToolResult(_) => {
            // These are handled specially in format_message
            String::new()
        }
    }
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

/// Parses the OpenAI API response and extracts the assistant message.
pub fn parse_response(response_body: &str) -> Result<Message, LlmError> {
    let parsed: serde_json::Value = serde_json::from_str(response_body)
        .map_err(|e| LlmError::new("PARSE_ERROR", format!("Failed to parse response: {}", e)))?;

    // Check for API error response
    if let Some(error_msg) = parsed["error"]["message"].as_str() {
        let error_type = parsed["error"]["type"].as_str().unwrap_or("api_error");
        return Err(LlmError::new(error_type, error_msg));
    }

    // OpenAI wraps response in choices[0].message
    let message = &parsed["choices"][0]["message"];

    if message.is_null() {
        return Err(LlmError::new(
            "PARSE_ERROR",
            "No message found in response",
        ));
    }

    let mut content_blocks: Vec<Content> = Vec::new();

    // Extract text content (check for null first)
    let content_field = &message["content"];
    if !content_field.is_null() {
        // serde_json properly unescapes JSON strings
        if let Some(text) = content_field.as_str() {
            if !text.is_empty() {
                content_blocks.push(Content::Text(text.to_string()));
            }
        }
    }

    // Extract tool calls
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

    if content_blocks.is_empty() {
        // OpenAI can return null content with tool calls, which we've handled
        // But if truly empty, that's an error
        return Err(LlmError::new(
            "PARSE_ERROR",
            "No content found in response",
        ));
    }

    Ok(Message::with_content(Role::Assistant, content_blocks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Tool;

    #[test]
    fn test_build_request_body_simple() {
        let messages = vec![Message::user("Hello")];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options, "gpt-4").unwrap();

        assert!(json.contains(r#""model":"gpt-4""#));
        assert!(json.contains(r#""role":"user""#));
        assert!(json.contains(r#""content":"Hello""#));
    }

    #[test]
    fn test_build_request_body_with_system() {
        let messages = vec![Message::system("You are helpful"), Message::user("Hello")];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options, "gpt-4").unwrap();

        assert!(json.contains(r#""role":"system""#));
        assert!(json.contains(r#""content":"You are helpful""#));
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

        let json = build_request_body(&messages, &options, "gpt-4").unwrap();

        assert!(json.contains(r#""tools":["#));
        assert!(json.contains(r#""type":"function""#));
        assert!(json.contains(r#""name":"get_weather""#));
        assert!(json.contains(r#""tool_choice":"auto""#));
    }

    #[test]
    fn test_build_request_body_with_tool_result() {
        let messages = vec![
            Message::user("What's the weather in SF?"),
            Message::with_content(
                Role::Assistant,
                vec![Content::ToolUse(ToolUse {
                    id: "call_123".to_string(),
                    name: "get_weather".to_string(),
                    input: r#"{"location":"San Francisco"}"#.to_string(),
                })],
            ),
            Message::tool_result("call_123", "72F and sunny", false),
        ];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options, "gpt-4").unwrap();

        assert!(json.contains(r#""tool_calls":["#));
        assert!(json.contains(r#""id":"call_123""#));
        assert!(json.contains(r#""role":"tool""#));
        assert!(json.contains(r#""tool_call_id":"call_123""#));
    }

    #[test]
    fn test_parse_response_text() {
        let response = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you?"
                },
                "finish_reason": "stop"
            }]
        }"#;

        let msg = parse_response(response).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        match &msg.content[0] {
            Content::Text(t) => assert_eq!(t, "Hello! How can I help you?"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_parse_response_tool_calls() {
        let response = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_456",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\":\"SF\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }"#;

        let msg = parse_response(response).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.content.len(), 1);

        match &msg.content[0] {
            Content::ToolUse(tu) => {
                assert_eq!(tu.id, "call_456");
                assert_eq!(tu.name, "get_weather");
            }
            _ => panic!("Expected tool use content"),
        }
    }

    #[test]
    fn test_parse_response_error() {
        let response = r#"{
            "error": {
                "message": "Invalid API key",
                "type": "invalid_request_error"
            }
        }"#;

        let err = parse_response(response).unwrap_err();
        assert_eq!(err.error_code, "invalid_request_error");
    }
}
