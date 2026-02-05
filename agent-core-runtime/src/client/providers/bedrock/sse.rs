//! Event stream parser for Bedrock streaming responses.
//!
//! Bedrock uses AWS event-stream encoding, not standard SSE.
//! The format is a binary protocol with headers and payload.

use crate::client::error::LlmError;
use crate::client::models::{ContentBlockType, StreamEvent, Usage};

// =============================================================================
// Constants
// =============================================================================

/// Prelude length in event-stream format (total_length:4 + headers_length:4 + crc:4).
const PRELUDE_LENGTH: usize = 12;

/// CRC length at the end of each message.
const MESSAGE_CRC_LENGTH: usize = 4;

/// Error code for event stream parsing errors.
const ERROR_EVENT_PARSE: &str = "EVENT_STREAM_PARSE_ERROR";

// =============================================================================
// Types
// =============================================================================

/// Parsed event from Bedrock event-stream.
#[derive(Debug)]
pub struct BedrockEvent {
    /// Event type (e.g., "messageStart", "contentBlockDelta", "messageStop").
    pub event_type: String,
    /// JSON payload.
    pub payload: String,
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
    /// Whether stream has finished.
    pub finished: bool,
}

// =============================================================================
// Public API
// =============================================================================

/// Parse event-stream encoded data from a buffer.
///
/// Returns parsed events and remaining bytes.
///
/// AWS event-stream format:
/// - 4 bytes: total message length (big-endian)
/// - 4 bytes: headers length (big-endian)
/// - 4 bytes: prelude CRC
/// - variable: headers
/// - variable: payload
/// - 4 bytes: message CRC
pub fn parse_event_stream(buffer: &[u8]) -> (Vec<BedrockEvent>, Vec<u8>) {
    let mut events = Vec::new();
    let mut offset = 0;

    while offset + PRELUDE_LENGTH <= buffer.len() {
        // Read total length
        let total_length = u32::from_be_bytes([
            buffer[offset],
            buffer[offset + 1],
            buffer[offset + 2],
            buffer[offset + 3],
        ]) as usize;

        // Check if we have the complete message
        if offset + total_length > buffer.len() {
            break;
        }

        // Read headers length
        let headers_length = u32::from_be_bytes([
            buffer[offset + 4],
            buffer[offset + 5],
            buffer[offset + 6],
            buffer[offset + 7],
        ]) as usize;

        // Skip prelude CRC (4 bytes at offset + 8)

        // Parse headers
        let headers_start = offset + PRELUDE_LENGTH;
        let headers_end = headers_start + headers_length;
        let headers_data = &buffer[headers_start..headers_end];
        let headers = parse_headers(headers_data);

        // Parse payload
        let payload_start = headers_end;
        let payload_end = offset + total_length - MESSAGE_CRC_LENGTH;
        let payload_data = &buffer[payload_start..payload_end];

        // Get event type from headers
        let event_type = headers
            .iter()
            .find(|(k, _)| k == ":event-type")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();

        // Convert payload to string
        if let Ok(payload) = std::str::from_utf8(payload_data) {
            events.push(BedrockEvent {
                event_type,
                payload: payload.to_string(),
            });
        }

        offset += total_length;
    }

    // Return remaining bytes
    let remaining = buffer[offset..].to_vec();

    (events, remaining)
}

/// Parse headers from header bytes.
fn parse_headers(data: &[u8]) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    let mut offset = 0;

    while offset < data.len() {
        // Header name length (1 byte)
        if offset >= data.len() {
            break;
        }
        let name_len = data[offset] as usize;
        offset += 1;

        // Header name
        if offset + name_len > data.len() {
            break;
        }
        let name = String::from_utf8_lossy(&data[offset..offset + name_len]).to_string();
        offset += name_len;

        // Header type (1 byte) - we only handle string type (7)
        if offset >= data.len() {
            break;
        }
        let header_type = data[offset];
        offset += 1;

        if header_type == 7 {
            // String type
            // Value length (2 bytes, big-endian)
            if offset + 2 > data.len() {
                break;
            }
            let value_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            offset += 2;

            // Value
            if offset + value_len > data.len() {
                break;
            }
            let value = String::from_utf8_lossy(&data[offset..offset + value_len]).to_string();
            offset += value_len;

            headers.push((name, value));
        } else {
            // Skip other header types (simplified)
            break;
        }
    }

    headers
}

/// Parse a Bedrock event into StreamEvents.
pub fn parse_stream_event(
    event: &BedrockEvent,
    state: &mut StreamState,
) -> Result<Vec<StreamEvent>, LlmError> {
    // Parse payload as JSON
    let json: serde_json::Value = serde_json::from_str(&event.payload)
        .map_err(|e| LlmError::new(ERROR_EVENT_PARSE, format!("Invalid JSON: {}", e)))?;

    let mut events = Vec::new();

    match event.event_type.as_str() {
        "messageStart" => {
            // Message starting - no events to emit yet
        }
        "contentBlockStart" => {
            // Check content block type
            let start = &json["start"];

            if start.get("text").is_some() {
                // Text block
                events.push(StreamEvent::ContentBlockStart {
                    index: state.block_index,
                    block_type: ContentBlockType::Text,
                });
                state.text_block_started = true;
            } else if let Some(tool_use) = start.get("toolUse") {
                // Tool use block
                let id = tool_use["toolUseId"].as_str().unwrap_or("").to_string();
                let name = tool_use["name"].as_str().unwrap_or("").to_string();

                events.push(StreamEvent::ContentBlockStart {
                    index: state.block_index,
                    block_type: ContentBlockType::ToolUse { id, name },
                });

                // Track this block index so we can close it at stream end if needed
                state.pending_tool_block_indices.push(state.block_index);
            }
        }
        "contentBlockDelta" => {
            let delta = &json["delta"];

            // Text delta
            if let Some(text) = delta["text"].as_str()
                && !text.is_empty()
            {
                events.push(StreamEvent::TextDelta {
                    index: state.block_index,
                    text: text.to_string(),
                });
            }

            // Tool input delta
            if let Some(tool_use) = delta.get("toolUse")
                && let Some(input) = tool_use["input"].as_str()
                && !input.is_empty()
            {
                events.push(StreamEvent::InputJsonDelta {
                    index: state.block_index,
                    json: input.to_string(),
                });
            }
        }
        "contentBlockStop" => {
            events.push(StreamEvent::ContentBlockStop {
                index: state.block_index,
            });
            // Remove from pending if it was a tool call
            state
                .pending_tool_block_indices
                .retain(|&idx| idx != state.block_index);
            state.block_index += 1;
            state.text_block_started = false;
        }
        "messageStop" => {
            state.finished = true;

            // Close any remaining open content blocks
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

            // Extract stop reason
            let stop_reason = json["stopReason"].as_str().map(|r| match r {
                "end_turn" => "end_turn".to_string(),
                "max_tokens" => "max_tokens".to_string(),
                "tool_use" => "tool_use".to_string(),
                "stop_sequence" => "stop_sequence".to_string(),
                other => other.to_string(),
            });

            events.push(StreamEvent::MessageDelta {
                stop_reason,
                usage: None,
            });
        }
        "metadata" => {
            // Extract usage information
            if let Some(usage) = json.get("usage") {
                let input_tokens = usage["inputTokens"].as_u64().unwrap_or(0) as u32;
                let output_tokens = usage["outputTokens"].as_u64().unwrap_or(0) as u32;

                events.push(StreamEvent::MessageDelta {
                    stop_reason: None,
                    usage: Some(Usage {
                        input_tokens,
                        output_tokens,
                    }),
                });
            }
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
    fn test_parse_headers() {
        // Simple header: name length (1), name, type (7 = string), value length (2), value
        let data = vec![
            11, // name length
            b':', b'e', b'v', b'e', b'n', b't', b'-', b't', b'y', b'p', b'e', // ":event-type"
            7,    // string type
            0, 12, // value length (12)
            b'm', b'e', b's', b's', b'a', b'g', b'e', b'S', b't', b'a', b'r',
            b't', // "messageStart"
        ];

        let headers = parse_headers(&data);
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, ":event-type");
        assert_eq!(headers[0].1, "messageStart");
    }

    #[test]
    fn test_parse_stream_event_text_delta() {
        let event = BedrockEvent {
            event_type: "contentBlockDelta".to_string(),
            payload: r#"{"contentBlockIndex":0,"delta":{"text":"Hello"}}"#.to_string(),
        };

        let mut state = StreamState::default();
        let events = parse_stream_event(&event, &mut state).unwrap();

        let has_text_delta = events
            .iter()
            .any(|e| matches!(e, StreamEvent::TextDelta { text, .. } if text == "Hello"));
        assert!(has_text_delta);
    }

    #[test]
    fn test_parse_stream_event_message_stop() {
        let event = BedrockEvent {
            event_type: "messageStop".to_string(),
            payload: r#"{"stopReason":"end_turn"}"#.to_string(),
        };

        let mut state = StreamState::default();
        let events = parse_stream_event(&event, &mut state).unwrap();

        let has_message_delta = events.iter().any(|e| {
            matches!(e, StreamEvent::MessageDelta { stop_reason: Some(reason), .. } if reason == "end_turn")
        });
        assert!(has_message_delta);
        assert!(state.finished);
    }
}
