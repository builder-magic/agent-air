//! Cohere API provider implementation.

mod sse;
mod types;

use async_stream::stream;
use futures::Stream;

use crate::client::error::LlmError;
use crate::client::http::HttpClient;
use crate::client::models::{Message, MessageOptions, StreamEvent};
use crate::client::traits::LlmProvider;
use std::future::Future;
use std::pin::Pin;

// =============================================================================
// Constants
// =============================================================================

/// Error code for SSE decoding errors.
const ERROR_SSE_DECODE: &str = "SSE_DECODE_ERROR";

/// Error message for invalid UTF-8 in stream.
const MSG_INVALID_UTF8: &str = "Invalid UTF-8 in stream";

// =============================================================================
// Provider
// =============================================================================

/// Cohere API provider.
///
/// Supports Cohere's chat API with tool calling capabilities.
pub struct CohereProvider {
    /// Cohere API key.
    api_key: String,
    /// Model identifier (e.g., "command-r-plus", "command-r").
    model: String,
}

impl CohereProvider {
    /// Create a new Cohere provider with API key and model.
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }

    /// Returns the model identifier.
    pub fn model(&self) -> &str {
        &self.model
    }
}

impl LlmProvider for CohereProvider {
    fn send_msg(
        &self,
        client: &HttpClient,
        messages: &[Message],
        options: &MessageOptions,
    ) -> Pin<Box<dyn Future<Output = Result<Message, LlmError>> + Send>> {
        // Clone data for the async block
        let client = client.clone();
        let api_key = self.api_key.clone();
        let model = options.model.as_deref().unwrap_or(&self.model).to_string();
        let messages = messages.to_vec();
        let options = options.clone();

        Box::pin(async move {
            // Build request body
            let body = types::build_request_body(&messages, &options, &model)?;

            // Get headers
            let headers = types::get_request_headers(&api_key)?;
            let headers_ref: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .collect();

            // Get the API URL
            let url = types::get_api_url();

            // Make the API call
            let response = client.post(&url, &headers_ref, &body).await?;

            // Parse and return the response
            types::parse_response(&response)
        })
    }

    fn send_msg_stream(
        &self,
        client: &HttpClient,
        messages: &[Message],
        options: &MessageOptions,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>, LlmError>> + Send>> {
        // Clone data for the async block
        let client = client.clone();
        let api_key = self.api_key.clone();
        let model = options.model.as_deref().unwrap_or(&self.model).to_string();
        let messages = messages.to_vec();
        let options = options.clone();

        Box::pin(async move {
            // Build streaming request body
            let body = types::build_streaming_request_body(&messages, &options, &model)?;

            // Get headers
            let headers = types::get_request_headers(&api_key)?;
            let headers_ref: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .collect();

            // Get the API URL
            let url = types::get_api_url();

            // Make the streaming API call
            let byte_stream = client.post_stream(&url, &headers_ref, &body).await?;

            // Convert byte stream to SSE events stream
            use futures::StreamExt;
            let event_stream = stream! {
                let mut buffer = String::new();
                let mut byte_stream = byte_stream;
                let mut message_started = false;
                let mut stream_state = sse::StreamState::default();

                while let Some(chunk_result) = byte_stream.next().await {
                    match chunk_result {
                        Ok(bytes) => {
                            // Append new bytes to buffer
                            if let Ok(text) = std::str::from_utf8(&bytes) {
                                buffer.push_str(text);
                            } else {
                                yield Err(LlmError::new(ERROR_SSE_DECODE, MSG_INVALID_UTF8));
                                break;
                            }

                            // Parse complete SSE events from buffer
                            let (events, remaining) = sse::parse_sse_chunk(&buffer);
                            buffer = remaining;

                            // Convert and yield each SSE event
                            for sse_event in events {
                                match sse::parse_stream_event(&sse_event, &mut stream_state) {
                                    Ok(stream_events) => {
                                        // Emit MessageStart on first content
                                        if !message_started && !stream_events.is_empty() {
                                            message_started = true;
                                            yield Ok(StreamEvent::MessageStart {
                                                message_id: String::new(),
                                                model: model.clone(),
                                            });
                                        }

                                        for stream_event in stream_events {
                                            yield Ok(stream_event);
                                        }
                                    }
                                    Err(e) => {
                                        yield Err(e);
                                        return;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            yield Err(e);
                            break;
                        }
                    }
                }

                // Emit MessageStop at the end
                if message_started {
                    yield Ok(StreamEvent::MessageStop);
                }
            };

            Ok(Box::pin(event_stream) as Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>)
        })
    }
}
