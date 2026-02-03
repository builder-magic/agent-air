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

/// Anthropic Claude API provider.
pub struct AnthropicProvider {
    /// Anthropic API key.
    pub api_key: String,
    /// Model identifier (e.g., "claude-3-5-sonnet-20241022").
    pub model: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with API key and model.
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

impl LlmProvider for AnthropicProvider {
    fn send_msg(
        &self,
        client: &HttpClient,
        messages: &[Message],
        options: &MessageOptions,
    ) -> Pin<Box<dyn Future<Output = Result<Message, LlmError>> + Send>> {
        // Clone data for the async block
        let client = client.clone();
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let messages = messages.to_vec();
        let options = options.clone();

        Box::pin(async move {
            // Build request body
            let body = types::build_request_body(&messages, &options, &model)?;

            // Get headers
            let headers = types::get_request_headers(&api_key);
            let headers_ref: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .collect();

            // Make the API call
            let response = client
                .post(types::get_api_url(), &headers_ref, &body)
                .await?;

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
        let model = self.model.clone();
        let messages = messages.to_vec();
        let options = options.clone();

        Box::pin(async move {
            // Build streaming request body
            let body = types::build_streaming_request_body(&messages, &options, &model)?;

            // Get headers
            let headers = types::get_request_headers(&api_key);
            let headers_ref: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .collect();

            // Make the streaming API call
            let byte_stream = client
                .post_stream(types::get_api_url(), &headers_ref, &body)
                .await?;

            // Convert byte stream to SSE events stream
            use futures::StreamExt;
            let event_stream = stream! {
                let mut buffer = String::new();
                let mut byte_stream = byte_stream;

                while let Some(chunk_result) = byte_stream.next().await {
                    match chunk_result {
                        Ok(bytes) => {
                            // Append new bytes to buffer
                            if let Ok(text) = std::str::from_utf8(&bytes) {
                                buffer.push_str(text);
                            } else {
                                yield Err(LlmError::new("SSE_DECODE_ERROR", "Invalid UTF-8 in stream"));
                                break;
                            }

                            // Parse complete SSE events from buffer
                            let (events, remaining) = sse::parse_sse_chunk(&buffer);
                            buffer = remaining;

                            // Convert and yield each SSE event
                            for sse_event in events {
                                match sse::parse_stream_event(&sse_event) {
                                    Ok(Some(stream_event)) => yield Ok(stream_event),
                                    Ok(None) => {} // Skip unknown events
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
            };

            Ok(Box::pin(event_stream) as Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>)
        })
    }
}