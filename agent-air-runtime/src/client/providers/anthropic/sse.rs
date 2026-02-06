// Server-Sent Events (SSE) parser for Anthropic streaming responses

use crate::client::error::LlmError;
use crate::client::models::{ContentBlockType, StreamEvent, Usage};

/// Error code for SSE parsing errors (consistent with other providers).
const ERROR_SSE_PARSE: &str = "SSE_PARSE_ERROR";

/// Parsed SSE event with event type and data.
#[derive(Debug)]
pub struct SseEvent {
    /// Event type (e.g., "message_start", "content_block_delta").
    pub event: String,
    /// JSON data payload.
    pub data: String,
}

/// Parse SSE lines from a buffer, returning events and remaining buffer
pub fn parse_sse_chunk(buffer: &str) -> (Vec<SseEvent>, String) {
    let mut events = Vec::new();
    let mut current_event = String::new();
    let mut current_data = String::new();
    let mut remaining = String::new();

    let lines: Vec<&str> = buffer.split('\n').collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Empty line marks end of event
        if line.is_empty() {
            if !current_event.is_empty() || !current_data.is_empty() {
                events.push(SseEvent {
                    event: current_event.clone(),
                    data: current_data.clone(),
                });
                current_event.clear();
                current_data.clear();
            }
        } else if let Some(event_type) = line.strip_prefix("event: ") {
            current_event = event_type.to_string();
        } else if let Some(data) = line.strip_prefix("data: ") {
            current_data = data.to_string();
        }
        // Ignore other lines (comments starting with :, etc.)

        i += 1;
    }

    // Keep incomplete event in remaining buffer
    if !current_event.is_empty() || !current_data.is_empty() {
        if !current_event.is_empty() {
            remaining.push_str("event: ");
            remaining.push_str(&current_event);
            remaining.push('\n');
        }
        if !current_data.is_empty() {
            remaining.push_str("data: ");
            remaining.push_str(&current_data);
        }
    }

    (events, remaining)
}

/// Parse an SSE event into a StreamEvent
pub fn parse_stream_event(sse: &SseEvent) -> Result<Option<StreamEvent>, LlmError> {
    match sse.event.as_str() {
        "message_start" => parse_message_start(&sse.data),
        "content_block_start" => parse_content_block_start(&sse.data),
        "content_block_delta" => parse_content_block_delta(&sse.data),
        "content_block_stop" => parse_content_block_stop(&sse.data),
        "message_delta" => parse_message_delta(&sse.data),
        "message_stop" => Ok(Some(StreamEvent::MessageStop)),
        "ping" => Ok(Some(StreamEvent::Ping)),
        "error" => parse_error(&sse.data),
        _ => Ok(None), // Unknown event type, skip
    }
}

fn parse_message_start(data: &str) -> Result<Option<StreamEvent>, LlmError> {
    // {"type":"message_start","message":{"id":"msg_...","model":"claude-3-...","content":[],...}}
    let json: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| LlmError::new(ERROR_SSE_PARSE, format!("Invalid JSON: {}", e)))?;

    let message = &json["message"];
    let message_id = message["id"].as_str().unwrap_or_default().to_string();
    let model = message["model"].as_str().unwrap_or_default().to_string();

    Ok(Some(StreamEvent::MessageStart { message_id, model }))
}

fn parse_content_block_start(data: &str) -> Result<Option<StreamEvent>, LlmError> {
    // {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}
    // {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"...","name":"..."}}
    let json: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| LlmError::new(ERROR_SSE_PARSE, format!("Invalid JSON: {}", e)))?;

    let index = json["index"].as_u64().unwrap_or(0) as usize;
    let content_block = &json["content_block"];
    let block_type_str = content_block["type"].as_str().unwrap_or("text");

    let block_type = match block_type_str {
        "text" => ContentBlockType::Text,
        "tool_use" => {
            let id = content_block["id"].as_str().unwrap_or_default().to_string();
            let name = content_block["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            ContentBlockType::ToolUse { id, name }
        }
        _ => ContentBlockType::Text,
    };

    Ok(Some(StreamEvent::ContentBlockStart { index, block_type }))
}

fn parse_content_block_delta(data: &str) -> Result<Option<StreamEvent>, LlmError> {
    // {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}
    // {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"..."}}
    let json: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| LlmError::new(ERROR_SSE_PARSE, format!("Invalid JSON: {}", e)))?;

    let index = json["index"].as_u64().unwrap_or(0) as usize;
    let delta = &json["delta"];
    let delta_type = delta["type"].as_str().unwrap_or("");

    match delta_type {
        "text_delta" => {
            // serde_json properly unescapes JSON strings (unlike jsonic)
            let text = delta["text"].as_str().unwrap_or_default().to_string();
            Ok(Some(StreamEvent::TextDelta { index, text }))
        }
        "input_json_delta" => {
            let json_str = delta["partial_json"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            Ok(Some(StreamEvent::InputJsonDelta {
                index,
                json: json_str,
            }))
        }
        _ => Ok(None),
    }
}

fn parse_content_block_stop(data: &str) -> Result<Option<StreamEvent>, LlmError> {
    // {"type":"content_block_stop","index":0}
    let json: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| LlmError::new(ERROR_SSE_PARSE, format!("Invalid JSON: {}", e)))?;

    let index = json["index"].as_u64().unwrap_or(0) as usize;
    Ok(Some(StreamEvent::ContentBlockStop { index }))
}

fn parse_message_delta(data: &str) -> Result<Option<StreamEvent>, LlmError> {
    // {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":15}}
    let json: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| LlmError::new(ERROR_SSE_PARSE, format!("Invalid JSON: {}", e)))?;

    let delta = &json["delta"];
    let stop_reason = delta["stop_reason"].as_str().map(|s| s.to_string());

    // Check if usage exists by looking for output_tokens
    let usage = if json["usage"]["output_tokens"].as_u64().is_some() {
        Some(Usage {
            input_tokens: json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
        })
    } else {
        None
    };

    Ok(Some(StreamEvent::MessageDelta { stop_reason, usage }))
}

fn parse_error(data: &str) -> Result<Option<StreamEvent>, LlmError> {
    // {"type":"error","error":{"type":"...","message":"..."}}
    let json: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| LlmError::new(ERROR_SSE_PARSE, format!("Invalid JSON: {}", e)))?;

    let error = &json["error"];
    let error_type = error["type"].as_str().unwrap_or("unknown");
    let error_message = error["message"].as_str().unwrap_or("Unknown error");

    Err(LlmError::new(error_type, error_message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sse_chunk() {
        let chunk =
            "event: message_start\ndata: {\"type\":\"message_start\"}\n\nevent: ping\ndata: {}\n\n";
        let (events, remaining) = parse_sse_chunk(chunk);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "message_start");
        assert_eq!(events[1].event, "ping");
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_parse_incomplete_chunk() {
        let chunk = "event: message_start\ndata: {\"test\":true}";
        let (events, remaining) = parse_sse_chunk(chunk);

        assert_eq!(events.len(), 0);
        assert!(remaining.contains("message_start"));
    }

    #[test]
    fn test_parse_text_delta() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let sse = SseEvent {
            event: "content_block_delta".to_string(),
            data: data.to_string(),
        };

        let event = parse_stream_event(&sse).unwrap().unwrap();
        if let StreamEvent::TextDelta { index, text } = event {
            assert_eq!(index, 0);
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected TextDelta");
        }
    }

    #[test]
    fn test_parse_tool_use_start() {
        let data = r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tool_123","name":"get_weather"}}"#;
        let sse = SseEvent {
            event: "content_block_start".to_string(),
            data: data.to_string(),
        };

        let event = parse_stream_event(&sse).unwrap().unwrap();
        if let StreamEvent::ContentBlockStart { index, block_type } = event {
            assert_eq!(index, 1);
            if let ContentBlockType::ToolUse { id, name } = block_type {
                assert_eq!(id, "tool_123");
                assert_eq!(name, "get_weather");
            } else {
                panic!("Expected ToolUse");
            }
        } else {
            panic!("Expected ContentBlockStart");
        }
    }
}
