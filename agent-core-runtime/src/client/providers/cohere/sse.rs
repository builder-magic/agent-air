//! Server-Sent Events (SSE) parser for Cohere streaming responses.

use crate::client::error::LlmError;
use crate::client::models::{ContentBlockType, StreamEvent, Usage};

// =============================================================================
// Constants
// =============================================================================

/// SSE data line prefix.
const SSE_DATA_PREFIX: &str = "data: ";

/// Error code for SSE parsing errors.
const ERROR_SSE_PARSE: &str = "SSE_PARSE_ERROR";

/// Default error message.
const MSG_UNKNOWN_ERROR: &str = "Unknown error";

// =============================================================================
// Types
// =============================================================================

/// Parsed SSE event with data payload.
#[derive(Debug)]
pub struct SseEvent {
    /// JSON data payload.
    pub data: String,
}

/// State tracker for streaming to handle content accumulation.
#[derive(Debug, Default, Clone)]
pub struct StreamState {
    /// Current content block index.
    pub block_index: usize,
    /// Whether we've emitted a text block start.
    pub text_block_started: bool,
    /// Block indices of tool calls that need to be closed at stream end.
    pub pending_tool_block_indices: Vec<usize>,
    /// Whether stream has ended.
    pub finished: bool,
}

// =============================================================================
// Public API
// =============================================================================

/// Parse SSE lines from a buffer, returning events and remaining buffer.
///
/// Cohere streams use SSE format with `data:` lines. Empty lines mark event boundaries.
pub fn parse_sse_chunk(buffer: &str) -> (Vec<SseEvent>, String) {
    let mut events = Vec::new();
    let mut current_data: Option<String> = None;

    let lines: Vec<&str> = buffer.split('\n').collect();

    for line in &lines {
        if line.is_empty() {
            // Empty line marks end of event
            if let Some(data) = current_data.take() {
                events.push(SseEvent { data });
            }
        } else if let Some(data) = line.strip_prefix(SSE_DATA_PREFIX) {
            current_data = Some(data.to_string());
        }
        // Ignore other lines (like event: or id:)
    }

    // Build remaining buffer only if there's incomplete data
    let remaining = if let Some(data) = current_data {
        format!("{}{}", SSE_DATA_PREFIX, data)
    } else {
        String::new()
    };

    (events, remaining)
}

/// Parse a Cohere SSE event into StreamEvents.
///
/// Cohere streaming events:
/// - `stream-start`: Start of stream
/// - `text-generation`: Text content delta
/// - `tool-calls-generation`: Tool call updates
/// - `tool-calls-chunk`: Tool argument deltas
/// - `stream-end`: End of stream with usage stats
pub fn parse_stream_event(
    sse: &SseEvent,
    state: &mut StreamState,
) -> Result<Vec<StreamEvent>, LlmError> {
    let json: serde_json::Value = serde_json::from_str(&sse.data)
        .map_err(|e| LlmError::new(ERROR_SSE_PARSE, format!("Invalid JSON: {}", e)))?;

    // Check for error response
    if let Some(error) = json.get("error") {
        let error_msg = error["message"].as_str().unwrap_or(MSG_UNKNOWN_ERROR);
        return Err(LlmError::new("COHERE_ERROR", error_msg));
    }

    let mut events = Vec::new();

    // Get event type
    let event_type = json["type"].as_str().unwrap_or("");

    match event_type {
        "message-start" => {
            // Start of message - no events to emit yet
        }
        "content-start" => {
            // Content block starting
            if !state.text_block_started {
                events.push(StreamEvent::ContentBlockStart {
                    index: state.block_index,
                    block_type: ContentBlockType::Text,
                });
                state.text_block_started = true;
            }
        }
        "content-delta" => {
            // Text content delta
            if let Some(delta) = json.get("delta") {
                if let Some(message) = delta.get("message") {
                    if let Some(content) = message.get("content") {
                        if let Some(text) = content.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                if !state.text_block_started {
                                    events.push(StreamEvent::ContentBlockStart {
                                        index: state.block_index,
                                        block_type: ContentBlockType::Text,
                                    });
                                    state.text_block_started = true;
                                }
                                events.push(StreamEvent::TextDelta {
                                    index: state.block_index,
                                    text: text.to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
        "content-end" => {
            // Content block ended
            if state.text_block_started {
                events.push(StreamEvent::ContentBlockStop {
                    index: state.block_index,
                });
                state.block_index += 1;
                state.text_block_started = false;
            }
        }
        "tool-call-start" => {
            // Tool call starting
            if let Some(delta) = json.get("delta") {
                if let Some(tool_call) = delta.get("tool_call") {
                    let id = tool_call["id"].as_str().unwrap_or("").to_string();
                    let name = tool_call["function"]["name"].as_str().unwrap_or("").to_string();

                    // Close text block if open
                    if state.text_block_started {
                        events.push(StreamEvent::ContentBlockStop {
                            index: state.block_index,
                        });
                        state.block_index += 1;
                        state.text_block_started = false;
                    }

                    let block_idx = state.block_index + state.pending_tool_block_indices.len();

                    // Emit tool use start
                    events.push(StreamEvent::ContentBlockStart {
                        index: block_idx,
                        block_type: ContentBlockType::ToolUse { id, name },
                    });

                    // Track this block index so we can close it at stream end if needed
                    state.pending_tool_block_indices.push(block_idx);
                }
            }
        }
        "tool-call-delta" => {
            // Tool call argument delta
            if let Some(delta) = json.get("delta") {
                if let Some(tool_call) = delta.get("tool_call") {
                    if let Some(args) = tool_call["function"]["arguments"].as_str() {
                        if !args.is_empty() {
                            // Emit delta for the most recent tool call
                            if let Some(&block_idx) = state.pending_tool_block_indices.last() {
                                events.push(StreamEvent::InputJsonDelta {
                                    index: block_idx,
                                    json: args.to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
        "tool-call-end" => {
            // Tool call ended - close the most recent one
            if let Some(block_idx) = state.pending_tool_block_indices.pop() {
                events.push(StreamEvent::ContentBlockStop { index: block_idx });
            }
        }
        "message-end" => {
            // End of message
            state.finished = true;

            // Close any open text block
            if state.text_block_started {
                events.push(StreamEvent::ContentBlockStop {
                    index: state.block_index,
                });
                state.text_block_started = false;
            }

            // Close any pending tool call blocks that weren't explicitly closed
            for &block_idx in &state.pending_tool_block_indices {
                events.push(StreamEvent::ContentBlockStop { index: block_idx });
            }
            state.pending_tool_block_indices.clear();

            // Extract usage if present
            let usage = json.get("delta")
                .and_then(|d| d.get("usage"))
                .map(|u| Usage {
                    input_tokens: u["billed_units"]["input_tokens"].as_u64().unwrap_or(0) as u32,
                    output_tokens: u["billed_units"]["output_tokens"].as_u64().unwrap_or(0) as u32,
                });

            // Determine stop reason
            let finish_reason = json.get("delta")
                .and_then(|d| d.get("finish_reason"))
                .and_then(|r| r.as_str())
                .map(|r| match r {
                    "COMPLETE" => "end_turn".to_string(),
                    "MAX_TOKENS" => "max_tokens".to_string(),
                    "TOOL_CALL" => "tool_use".to_string(),
                    other => other.to_string(),
                });

            events.push(StreamEvent::MessageDelta {
                stop_reason: finish_reason,
                usage,
            });
        }
        _ => {
            // Unknown event type - ignore
        }
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sse_chunk() {
        let chunk = "data: {\"test\":true}\n\ndata: {\"test\":false}\n\n";
        let (events, remaining) = parse_sse_chunk(chunk);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "{\"test\":true}");
        assert_eq!(events[1].data, "{\"test\":false}");
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_parse_incomplete_chunk() {
        let chunk = "data: {\"test\":true}";
        let (events, remaining) = parse_sse_chunk(chunk);

        assert_eq!(events.len(), 0);
        assert!(remaining.contains("{\"test\":true}"));
    }

    #[test]
    fn test_parse_text_delta() {
        let data = r#"{"type":"content-delta","delta":{"message":{"content":{"text":"Hello"}}}}"#;
        let sse = SseEvent {
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        let events = parse_stream_event(&sse, &mut state).unwrap();

        let has_text_delta = events.iter().any(|e| {
            matches!(e, StreamEvent::TextDelta { text, .. } if text == "Hello")
        });
        assert!(has_text_delta);
    }

    #[test]
    fn test_parse_message_end() {
        let data = r#"{"type":"message-end","delta":{"finish_reason":"COMPLETE","usage":{"billed_units":{"input_tokens":10,"output_tokens":20}}}}"#;
        let sse = SseEvent {
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        let events = parse_stream_event(&sse, &mut state).unwrap();

        let has_message_delta = events.iter().any(|e| {
            matches!(e, StreamEvent::MessageDelta { stop_reason: Some(reason), usage: Some(u) }
                if reason == "end_turn" && u.output_tokens == 20)
        });
        assert!(has_message_delta);
        assert!(state.finished);
    }

    #[test]
    fn test_parse_error() {
        let data = r#"{"error":{"message":"Invalid API key"}}"#;
        let sse = SseEvent {
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        let result = parse_stream_event(&sse, &mut state);

        assert!(result.is_err());
    }
}
