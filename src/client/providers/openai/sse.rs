//! Server-Sent Events (SSE) parser for OpenAI streaming responses.

use crate::client::error::LlmError;
use crate::client::models::{ContentBlockType, StreamEvent, Usage};

// =============================================================================
// Constants
// =============================================================================

/// SSE data line prefix.
const SSE_DATA_PREFIX: &str = "data: ";

/// OpenAI stream termination marker.
const STREAM_DONE_MARKER: &str = "[DONE]";

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
    /// JSON data payload (or "[DONE]" for stream end).
    pub data: String,
}

/// State tracker for streaming to handle tool call accumulation.
#[derive(Debug, Default, Clone)]
pub struct StreamState {
    /// Current content block index.
    pub block_index: usize,
    /// Whether we've emitted a text block start.
    pub text_block_started: bool,
    /// Pending tool calls being accumulated (id -> (name, args_buffer)).
    pub pending_tool_calls: Vec<PendingToolCall>,
    /// Model name from the first chunk.
    pub model: Option<String>,
    /// Message ID from the first chunk.
    pub message_id: Option<String>,
}

/// Tracks a tool call being accumulated across streaming chunks.
#[derive(Debug, Clone)]
pub struct PendingToolCall {
    /// Tool call index in the response.
    pub index: usize,
    /// Content block index for events.
    pub block_index: usize,
    /// Tool call ID.
    pub id: String,
    /// Function name.
    pub name: String,
    /// Accumulated arguments.
    pub arguments: String,
    /// Whether start event has been emitted.
    pub started: bool,
}

// =============================================================================
// Public API
// =============================================================================

/// Parse SSE lines from a buffer, returning events and remaining buffer.
///
/// OpenAI streams use SSE format with `data:` lines. Empty lines mark event boundaries.
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
        // Ignore other lines
    }

    // Build remaining buffer only if there's incomplete data
    let remaining = if let Some(data) = current_data {
        format!("{}{}", SSE_DATA_PREFIX, data)
    } else {
        String::new()
    };

    (events, remaining)
}

/// Parse an OpenAI SSE event into StreamEvents.
///
/// OpenAI streaming format:
/// - First chunk: Contains model, id, and possibly initial content
/// - Content chunks: `choices[0].delta.content` for text
/// - Tool call chunks: `choices[0].delta.tool_calls[n]` for function calls
/// - Final chunk: `choices[0].finish_reason` is set
/// - Stream end: `data: [DONE]`
pub fn parse_stream_event(
    sse: &SseEvent,
    state: &mut StreamState,
) -> Result<Vec<StreamEvent>, LlmError> {
    // Check for stream end marker
    if sse.data == STREAM_DONE_MARKER {
        return Ok(vec![StreamEvent::MessageStop]);
    }

    let json: serde_json::Value = serde_json::from_str(&sse.data)
        .map_err(|e| LlmError::new(ERROR_SSE_PARSE, format!("Invalid JSON: {}", e)))?;

    // Check for error response
    if let Some(error) = json.get("error") {
        let error_type = error["type"].as_str().unwrap_or("api_error");
        let error_msg = error["message"].as_str().unwrap_or(MSG_UNKNOWN_ERROR);
        return Err(LlmError::new(error_type, error_msg));
    }

    let mut events = Vec::new();

    // Extract message metadata from first chunk
    if state.message_id.is_none() {
        if let Some(id) = json["id"].as_str() {
            state.message_id = Some(id.to_string());
        }
        if let Some(model) = json["model"].as_str() {
            state.model = Some(model.to_string());
        }

        // Emit MessageStart
        events.push(StreamEvent::MessageStart {
            message_id: state.message_id.clone().unwrap_or_default(),
            model: state.model.clone().unwrap_or_default(),
        });
    }

    // Process choices
    if let Some(choices) = json["choices"].as_array() {
        if let Some(choice) = choices.first() {
            let delta = &choice["delta"];

            // Handle text content
            if let Some(content) = delta["content"].as_str() {
                if !content.is_empty() {
                    // Start text block if not started
                    if !state.text_block_started {
                        events.push(StreamEvent::ContentBlockStart {
                            index: state.block_index,
                            block_type: ContentBlockType::Text,
                        });
                        state.text_block_started = true;
                    }

                    events.push(StreamEvent::TextDelta {
                        index: state.block_index,
                        text: content.to_string(),
                    });
                }
            }

            // Handle tool calls
            if let Some(tool_calls) = delta["tool_calls"].as_array() {
                for tc in tool_calls {
                    let tc_index = tc["index"].as_u64().unwrap_or(0) as usize;

                    // Find or create pending tool call
                    let pending = if let Some(p) = state
                        .pending_tool_calls
                        .iter_mut()
                        .find(|p| p.index == tc_index)
                    {
                        p
                    } else {
                        // Close text block if open
                        if state.text_block_started {
                            events.push(StreamEvent::ContentBlockStop {
                                index: state.block_index,
                            });
                            state.block_index += 1;
                            state.text_block_started = false;
                        }

                        // Create new pending tool call
                        let block_idx = state.block_index + state.pending_tool_calls.len();
                        state.pending_tool_calls.push(PendingToolCall {
                            index: tc_index,
                            block_index: block_idx,
                            id: String::new(),
                            name: String::new(),
                            arguments: String::new(),
                            started: false,
                        });
                        state.pending_tool_calls.last_mut().unwrap()
                    };

                    // Update tool call ID
                    if let Some(id) = tc["id"].as_str() {
                        pending.id = id.to_string();
                    }

                    // Update function name
                    if let Some(name) = tc["function"]["name"].as_str() {
                        pending.name = name.to_string();
                    }

                    // Accumulate arguments
                    if let Some(args) = tc["function"]["arguments"].as_str() {
                        pending.arguments.push_str(args);
                    }

                    // Emit start event if we have enough info and haven't started
                    if !pending.started && !pending.id.is_empty() && !pending.name.is_empty() {
                        events.push(StreamEvent::ContentBlockStart {
                            index: pending.block_index,
                            block_type: ContentBlockType::ToolUse {
                                id: pending.id.clone(),
                                name: pending.name.clone(),
                            },
                        });
                        pending.started = true;
                    }

                    // Emit argument delta if we have arguments and have started
                    if pending.started {
                        if let Some(args) = tc["function"]["arguments"].as_str() {
                            if !args.is_empty() {
                                events.push(StreamEvent::InputJsonDelta {
                                    index: pending.block_index,
                                    json: args.to_string(),
                                });
                            }
                        }
                    }
                }
            }

            // Handle finish reason
            if let Some(finish_reason) = choice["finish_reason"].as_str() {
                // Close text block if open
                if state.text_block_started {
                    events.push(StreamEvent::ContentBlockStop {
                        index: state.block_index,
                    });
                    state.text_block_started = false;
                }

                // Close all pending tool calls
                for pending in &state.pending_tool_calls {
                    if pending.started {
                        events.push(StreamEvent::ContentBlockStop {
                            index: pending.block_index,
                        });
                    }
                }

                // Map finish reason
                let stop_reason = Some(match finish_reason {
                    "stop" => "end_turn".to_string(),
                    "length" => "max_tokens".to_string(),
                    "tool_calls" => "tool_use".to_string(),
                    "content_filter" => "content_filter".to_string(),
                    other => other.to_string(),
                });

                // Extract usage if present
                let usage = json.get("usage").map(|u| Usage {
                    input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                    output_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
                });

                events.push(StreamEvent::MessageDelta { stop_reason, usage });
            }
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
    fn test_parse_done_marker() {
        let sse = SseEvent {
            data: STREAM_DONE_MARKER.to_string(),
        };

        let mut state = StreamState::default();
        let events = parse_stream_event(&sse, &mut state).unwrap();

        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], StreamEvent::MessageStop));
    }

    #[test]
    fn test_parse_first_chunk() {
        let data = r#"{"id":"chatcmpl-123","model":"gpt-4","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#;
        let sse = SseEvent {
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        let events = parse_stream_event(&sse, &mut state).unwrap();

        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::MessageStart { .. })));
        assert_eq!(state.message_id, Some("chatcmpl-123".to_string()));
        assert_eq!(state.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_parse_content_delta() {
        let data =
            r#"{"id":"chatcmpl-123","model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let sse = SseEvent {
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        state.message_id = Some("chatcmpl-123".to_string());
        state.model = Some("gpt-4".to_string());

        let events = parse_stream_event(&sse, &mut state).unwrap();

        let has_text_delta = events.iter().any(|e| {
            matches!(e, StreamEvent::TextDelta { text, .. } if text == "Hello")
        });
        assert!(has_text_delta);
    }

    #[test]
    fn test_parse_tool_call() {
        let data = r#"{"id":"chatcmpl-123","model":"gpt-4","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":"{\"loc"}}]},"finish_reason":null}]}"#;
        let sse = SseEvent {
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        state.message_id = Some("chatcmpl-123".to_string());
        state.model = Some("gpt-4".to_string());

        let events = parse_stream_event(&sse, &mut state).unwrap();

        let has_tool_start = events.iter().any(|e| {
            matches!(e, StreamEvent::ContentBlockStart { block_type: ContentBlockType::ToolUse { name, .. }, .. } if name == "get_weather")
        });
        assert!(has_tool_start);
    }

    #[test]
    fn test_parse_finish_reason() {
        let data = r#"{"id":"chatcmpl-123","model":"gpt-4","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        let sse = SseEvent {
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        state.message_id = Some("chatcmpl-123".to_string());
        state.model = Some("gpt-4".to_string());

        let events = parse_stream_event(&sse, &mut state).unwrap();

        let has_message_delta = events.iter().any(|e| {
            matches!(e, StreamEvent::MessageDelta { stop_reason: Some(reason), .. } if reason == "end_turn")
        });
        assert!(has_message_delta);
    }

    #[test]
    fn test_parse_error() {
        let data = r#"{"error":{"type":"invalid_request_error","message":"Invalid API key"}}"#;
        let sse = SseEvent {
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        let result = parse_stream_event(&sse, &mut state);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_code, "invalid_request_error");
    }
}
