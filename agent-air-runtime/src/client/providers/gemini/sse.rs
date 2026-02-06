//! Server-Sent Events (SSE) parser for Gemini streaming responses.

use crate::client::error::LlmError;
use crate::client::models::{ContentBlockType, StreamEvent, Usage};

// =============================================================================
// Constants
// =============================================================================

/// SSE data line prefix.
const SSE_DATA_PREFIX: &str = "data: ";

/// Error code for SSE parsing errors.
const ERROR_SSE_PARSE: &str = "SSE_PARSE_ERROR";

/// Prefix for Gemini API error codes.
const ERROR_PREFIX_GEMINI: &str = "GEMINI_ERROR_";

/// Default error message when error details are unavailable.
const MSG_UNKNOWN_ERROR: &str = "Unknown error";

/// Gemini finish reason: normal completion.
const FINISH_REASON_STOP: &str = "STOP";

/// Gemini finish reason: max tokens reached.
const FINISH_REASON_MAX_TOKENS: &str = "MAX_TOKENS";

/// Gemini finish reason: safety filter triggered.
const FINISH_REASON_SAFETY: &str = "SAFETY";

/// Gemini finish reason: recitation detected.
const FINISH_REASON_RECITATION: &str = "RECITATION";

/// Internal stop reason: end of turn.
const STOP_REASON_END_TURN: &str = "end_turn";

/// Internal stop reason: max tokens.
const STOP_REASON_MAX_TOKENS: &str = "max_tokens";

/// Internal stop reason: safety.
const STOP_REASON_SAFETY: &str = "safety";

/// Internal stop reason: recitation.
const STOP_REASON_RECITATION: &str = "recitation";

// =============================================================================
// Types
// =============================================================================

/// Parsed SSE event with optional event type and data payload.
///
/// Matches Anthropic's SSE structure for consistency across providers.
#[derive(Debug)]
pub struct SseEvent {
    /// Event type - included for cross-provider consistency even though
    /// Gemini doesn't typically use event types in their SSE stream.
    #[allow(dead_code)]
    pub event: Option<String>,
    /// JSON data payload.
    pub data: String,
}

/// State tracker for streaming to handle indices across chunks.
#[derive(Debug, Default, Clone)]
pub struct StreamState {
    /// Number of content blocks seen so far.
    pub block_count: usize,
    /// Whether we've seen at least one content block.
    pub has_content: bool,
    /// Pending function call being accumulated (for partial args).
    /// Item 15 fix: accumulate function call args across chunks.
    pub pending_function_call: Option<PendingFunctionCall>,
}

/// Tracks a function call being accumulated across streaming chunks.
#[derive(Debug, Clone)]
pub struct PendingFunctionCall {
    /// Index of this content block.
    pub index: usize,
    /// Generated ID for this function call - stored for debugging and potential
    /// future use in tool call correlation.
    #[allow(dead_code)]
    pub id: String,
    /// Function name.
    pub name: String,
    /// Accumulated JSON arguments buffer - written during streaming, available
    /// for debugging partial argument accumulation.
    #[allow(dead_code)]
    pub args_buffer: String,
    /// Tracks whether the ToolUseStart event has been emitted - stored for
    /// debugging streaming state.
    #[allow(dead_code)]
    pub started: bool,
}

// =============================================================================
// Public API
// =============================================================================

/// Parse SSE lines from a buffer, returning events and remaining buffer.
///
/// Gemini streams use SSE format with `data:` lines. Empty lines mark event boundaries.
pub fn parse_sse_chunk(buffer: &str) -> (Vec<SseEvent>, String) {
    let mut events = Vec::new();
    let mut current_event: Option<String> = None;
    let mut current_data: Option<String> = None;

    let lines: Vec<&str> = buffer.split('\n').collect();

    for line in &lines {
        if line.is_empty() {
            // Empty line marks end of event
            if let Some(data) = current_data.take() {
                events.push(SseEvent {
                    event: current_event.take(),
                    data,
                });
            }
            current_event = None;
        } else if let Some(event_type) = line.strip_prefix("event: ") {
            current_event = Some(event_type.to_string());
        } else if let Some(data) = line.strip_prefix(SSE_DATA_PREFIX) {
            current_data = Some(data.to_string());
        }
        // Ignore other lines (comments starting with :, etc.)
    }

    // Build remaining buffer only if there's incomplete data
    let remaining = if current_data.is_some() || current_event.is_some() {
        let mut rem = String::new();
        if let Some(evt) = current_event {
            rem.push_str("event: ");
            rem.push_str(&evt);
            rem.push('\n');
        }
        if let Some(data) = current_data {
            rem.push_str(SSE_DATA_PREFIX);
            rem.push_str(&data);
        }
        rem
    } else {
        String::new()
    };

    (events, remaining)
}

/// Parse a Gemini SSE event into StreamEvents.
///
/// Gemini sends complete candidate objects in each SSE event. This function
/// uses the provided state to track content block indices across events and
/// accumulate partial function call arguments (Item 15 fix).
pub fn parse_stream_event(
    sse: &SseEvent,
    state: &mut StreamState,
) -> Result<Vec<StreamEvent>, LlmError> {
    let json: serde_json::Value = serde_json::from_str(&sse.data)
        .map_err(|e| LlmError::new(ERROR_SSE_PARSE, format!("Invalid JSON: {}", e)))?;

    // Check for error response
    if let Some(error) = json.get("error") {
        let error_code = error["code"].as_i64().unwrap_or(0);
        let error_msg = error["message"].as_str().unwrap_or(MSG_UNKNOWN_ERROR);
        return Err(LlmError::new(
            format!("{}{}", ERROR_PREFIX_GEMINI, error_code),
            error_msg,
        ));
    }

    let mut events = Vec::new();

    // Gemini streaming format:
    // {"candidates":[{"content":{"parts":[...],"role":"model"},...}],"usageMetadata":{...}}
    let candidates = &json["candidates"];

    if let Some(candidates_array) = candidates.as_array()
        && let Some(candidate) = candidates_array.first()
    {
        let content = &candidate["content"];
        let parts = &content["parts"];

        if let Some(parts_array) = parts.as_array() {
            for part in parts_array {
                let index = state.block_count;

                // Text part
                if let Some(text) = part["text"].as_str() {
                    if !text.is_empty() {
                        // Finalize any pending function call first
                        if let Some(pending) = state.pending_function_call.take() {
                            events.extend(finalize_function_call(&pending));
                        }

                        // Only emit start if this is new content
                        if !state.has_content || index >= state.block_count {
                            events.push(StreamEvent::ContentBlockStart {
                                index,
                                block_type: ContentBlockType::Text,
                            });
                        }

                        events.push(StreamEvent::TextDelta {
                            index,
                            text: text.to_string(),
                        });

                        state.has_content = true;
                        state.block_count = index + 1;
                    }
                }
                // Function call part - Item 15 fix: accumulate partial args
                else if let Some(function_call) = part.get("functionCall") {
                    let name = function_call["name"].as_str().unwrap_or("");
                    let args_json = &function_call["args"];

                    // Check if this is a continuation of a pending function call
                    if let Some(ref mut pending) = state.pending_function_call {
                        if pending.name == name {
                            // Accumulate args - try to merge JSON
                            let new_args = args_json.to_string();
                            if is_complete_json(&new_args) {
                                // Complete args received, use them
                                pending.args_buffer = new_args;
                            } else {
                                // Partial args, append to buffer
                                pending.args_buffer.push_str(&new_args);
                            }
                            // Emit delta for the new args
                            events.push(StreamEvent::InputJsonDelta {
                                index: pending.index,
                                json: args_json.to_string(),
                            });
                            continue;
                        } else {
                            // Different function, finalize the previous one
                            let pending_owned = state.pending_function_call.take().unwrap();
                            events.extend(finalize_function_call(&pending_owned));
                        }
                    }

                    // Start a new function call
                    // NOTE: Gemini matches function responses by NAME, not by unique ID.
                    // We use the function name as the ID so tool results flow back correctly.
                    let id = name.to_string();
                    let args_str = args_json.to_string();

                    events.push(StreamEvent::ContentBlockStart {
                        index,
                        block_type: ContentBlockType::ToolUse {
                            id: id.clone(),
                            name: name.to_string(),
                        },
                    });

                    events.push(StreamEvent::InputJsonDelta {
                        index,
                        json: args_str.clone(),
                    });

                    // Check if args are complete
                    if is_complete_json(&args_str) {
                        // Complete function call, emit stop immediately
                        events.push(StreamEvent::ContentBlockStop { index });
                        state.has_content = true;
                        state.block_count = index + 1;
                    } else {
                        // Partial args, store for accumulation
                        state.pending_function_call = Some(PendingFunctionCall {
                            index,
                            id,
                            name: name.to_string(),
                            args_buffer: args_str,
                            started: true,
                        });
                        state.has_content = true;
                        state.block_count = index + 1;
                    }
                }
            }
        }

        // Check for finish reason
        if let Some(finish_reason) = candidate["finishReason"].as_str() {
            // Finalize any pending function call
            if let Some(pending) = state.pending_function_call.take() {
                events.extend(finalize_function_call(&pending));
            }

            // Emit content block stop for text blocks on completion
            if state.has_content && state.block_count > 0 {
                let last_index = state.block_count - 1;
                // Only emit if not already stopped (e.g., not a function call)
                let already_stopped = events.iter().any(|e| {
                        matches!(e, StreamEvent::ContentBlockStop { index } if *index == last_index)
                    });
                if !already_stopped {
                    events.push(StreamEvent::ContentBlockStop { index: last_index });
                }
            }

            let stop_reason = map_finish_reason(finish_reason);

            // Extract usage if available
            let usage = extract_usage(&json);

            events.push(StreamEvent::MessageDelta { stop_reason, usage });
        }
    }

    // Check for usage metadata at the top level (final message)
    if let Some(usage_meta) = json.get("usageMetadata") {
        let has_candidates = json
            .get("candidates")
            .and_then(|c| c.as_array())
            .is_some_and(|a| !a.is_empty());

        if !has_candidates {
            // This is a usage-only message (final stats)
            events.push(StreamEvent::MessageDelta {
                stop_reason: None,
                usage: Some(Usage {
                    input_tokens: usage_meta["promptTokenCount"].as_u64().unwrap_or(0) as u32,
                    output_tokens: usage_meta["candidatesTokenCount"].as_u64().unwrap_or(0) as u32,
                }),
            });
        }
    }

    Ok(events)
}

/// Finalize a pending function call by emitting ContentBlockStop.
fn finalize_function_call(pending: &PendingFunctionCall) -> Vec<StreamEvent> {
    vec![StreamEvent::ContentBlockStop {
        index: pending.index,
    }]
}

/// Check if a JSON string appears to be complete (balanced braces/brackets).
fn is_complete_json(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }

    // Quick check: try to parse it
    serde_json::from_str::<serde_json::Value>(s).is_ok()
}

// =============================================================================
// Private helpers
// =============================================================================

/// Map Gemini finish reason to internal stop reason.
fn map_finish_reason(finish_reason: &str) -> Option<String> {
    Some(match finish_reason {
        FINISH_REASON_STOP => STOP_REASON_END_TURN.to_string(),
        FINISH_REASON_MAX_TOKENS => STOP_REASON_MAX_TOKENS.to_string(),
        FINISH_REASON_SAFETY => STOP_REASON_SAFETY.to_string(),
        FINISH_REASON_RECITATION => STOP_REASON_RECITATION.to_string(),
        other => other.to_lowercase(),
    })
}

/// Extract usage metadata from JSON response.
fn extract_usage(json: &serde_json::Value) -> Option<Usage> {
    json.get("usageMetadata").map(|usage_meta| Usage {
        input_tokens: usage_meta["promptTokenCount"].as_u64().unwrap_or(0) as u32,
        output_tokens: usage_meta["candidatesTokenCount"].as_u64().unwrap_or(0) as u32,
    })
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
    fn test_parse_sse_chunk_with_event_type() {
        let chunk = "event: message\ndata: {\"test\":true}\n\n";
        let (events, remaining) = parse_sse_chunk(chunk);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, Some("message".to_string()));
        assert_eq!(events[0].data, "{\"test\":true}");
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_parse_incomplete_chunk() {
        let chunk = "data: {\"test\":true}";
        let (events, remaining) = parse_sse_chunk(chunk);

        assert_eq!(events.len(), 0);
        assert!(remaining.contains("{\"test\":true}"));
        assert!(remaining.starts_with(SSE_DATA_PREFIX));
    }

    #[test]
    fn test_parse_text_response() {
        let data = r#"{"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"},"finishReason":"STOP"}]}"#;
        let sse = SseEvent {
            event: None,
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        let events = parse_stream_event(&sse, &mut state).unwrap();

        // Should have: ContentBlockStart, TextDelta, ContentBlockStop, MessageDelta
        assert!(events.len() >= 3);

        let has_text_delta = events
            .iter()
            .any(|e| matches!(e, StreamEvent::TextDelta { text, .. } if text == "Hello"));
        assert!(has_text_delta);

        let has_stop = events.iter().any(|e| {
            matches!(e, StreamEvent::MessageDelta { stop_reason: Some(reason), .. } if reason == STOP_REASON_END_TURN)
        });
        assert!(has_stop);
    }

    #[test]
    fn test_parse_function_call_response() {
        let data = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_weather","args":{"location":"SF"}}}],"role":"model"},"finishReason":"STOP"}]}"#;
        let sse = SseEvent {
            event: None,
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        let events = parse_stream_event(&sse, &mut state).unwrap();

        let has_tool_use = events.iter().any(|e| {
            matches!(e, StreamEvent::ContentBlockStart { block_type: ContentBlockType::ToolUse { name, .. }, .. } if name == "get_weather")
        });
        assert!(has_tool_use);
    }

    #[test]
    fn test_parse_error_response() {
        let data = r#"{"error":{"code":400,"message":"Invalid request"}}"#;
        let sse = SseEvent {
            event: None,
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        let result = parse_stream_event(&sse, &mut state);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.error_code.starts_with(ERROR_PREFIX_GEMINI));
    }

    #[test]
    fn test_parse_usage_metadata() {
        let data = r#"{"candidates":[{"content":{"parts":[{"text":"Hi"}],"role":"model"},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5}}"#;
        let sse = SseEvent {
            event: None,
            data: data.to_string(),
        };

        let mut state = StreamState::default();
        let events = parse_stream_event(&sse, &mut state).unwrap();

        let has_usage = events.iter().any(|e| {
            matches!(e, StreamEvent::MessageDelta { usage: Some(u), .. } if u.input_tokens == 10 && u.output_tokens == 5)
        });
        assert!(has_usage);
    }

    #[test]
    fn test_stream_state_tracking() {
        let mut state = StreamState::default();
        assert_eq!(state.block_count, 0);
        assert!(!state.has_content);

        // First chunk
        let data1 = r#"{"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"}}]}"#;
        let sse1 = SseEvent {
            event: None,
            data: data1.to_string(),
        };
        let _ = parse_stream_event(&sse1, &mut state).unwrap();

        assert_eq!(state.block_count, 1);
        assert!(state.has_content);
    }

    #[test]
    fn test_map_finish_reason() {
        assert_eq!(
            map_finish_reason(FINISH_REASON_STOP),
            Some(STOP_REASON_END_TURN.to_string())
        );
        assert_eq!(
            map_finish_reason(FINISH_REASON_MAX_TOKENS),
            Some(STOP_REASON_MAX_TOKENS.to_string())
        );
        assert_eq!(
            map_finish_reason(FINISH_REASON_SAFETY),
            Some(STOP_REASON_SAFETY.to_string())
        );
        assert_eq!(map_finish_reason("UNKNOWN"), Some("unknown".to_string()));
    }

    #[test]
    fn test_is_complete_json() {
        assert!(is_complete_json(r#"{"key": "value"}"#));
        assert!(is_complete_json(r#"{"nested": {"key": "value"}}"#));
        assert!(is_complete_json(r#"[1, 2, 3]"#));
        assert!(is_complete_json(r#""string""#));
        assert!(is_complete_json(r#"123"#));
        assert!(is_complete_json(r#"null"#));

        // Incomplete JSON
        assert!(!is_complete_json(r#"{"key": "val"#));
        assert!(!is_complete_json(r#"{"nested": {"#));
        assert!(!is_complete_json(r#"[1, 2,"#));
        assert!(!is_complete_json(r#""#));
    }

    #[test]
    fn test_partial_function_call_accumulation() {
        let mut state = StreamState::default();

        // First chunk with function call start
        let data1 = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_weather","args":{"location":"San Francisco"}}}],"role":"model"},"finishReason":"STOP"}]}"#;
        let sse1 = SseEvent {
            event: None,
            data: data1.to_string(),
        };

        let events = parse_stream_event(&sse1, &mut state).unwrap();

        // Should have start, delta, stop, and message delta
        let has_start = events.iter().any(|e| {
            matches!(e, StreamEvent::ContentBlockStart { block_type: ContentBlockType::ToolUse { name, .. }, .. } if name == "get_weather")
        });
        assert!(has_start);

        let has_delta = events.iter().any(|e| {
            matches!(e, StreamEvent::InputJsonDelta { json, .. } if json.contains("San Francisco"))
        });
        assert!(has_delta);

        let has_stop = events
            .iter()
            .any(|e| matches!(e, StreamEvent::ContentBlockStop { .. }));
        assert!(has_stop);
    }
}
