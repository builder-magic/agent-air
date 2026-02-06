//! Amazon Bedrock API provider implementation.
//!
//! Uses the Converse API for chat completions with AWS SigV4 authentication.

mod signing;
mod sse;
mod types;

use async_stream::stream;
use futures::Stream;

use crate::client::error::LlmError;
use crate::client::http::HttpClient;
use crate::client::models::{Message, MessageOptions, StreamEvent};
use crate::client::traits::{LlmProvider, StreamMsgFuture};
use std::future::Future;
use std::pin::Pin;

// =============================================================================
// Provider
// =============================================================================

/// AWS Bedrock credentials.
#[derive(Clone)]
pub struct BedrockCredentials {
    /// AWS access key ID.
    pub access_key_id: String,
    /// AWS secret access key.
    pub secret_access_key: String,
    /// Optional session token for temporary credentials.
    pub session_token: Option<String>,
}

impl BedrockCredentials {
    /// Create new credentials with access key and secret.
    pub fn new(access_key_id: impl Into<String>, secret_access_key: impl Into<String>) -> Self {
        Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
        }
    }

    /// Create new credentials with session token (for temporary/assumed role credentials).
    pub fn with_session_token(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        session_token: impl Into<String>,
    ) -> Self {
        Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: Some(session_token.into()),
        }
    }
}

/// Amazon Bedrock API provider.
///
/// Uses the Converse API for chat completions.
/// Requires AWS credentials and region configuration.
pub struct BedrockProvider {
    /// AWS credentials.
    credentials: BedrockCredentials,
    /// AWS region (e.g., "us-east-1").
    region: String,
    /// Model identifier (e.g., "anthropic.claude-3-sonnet-20240229-v1:0").
    model: String,
}

impl BedrockProvider {
    /// Create a new Bedrock provider.
    ///
    /// # Arguments
    /// * `credentials` - AWS credentials
    /// * `region` - AWS region (e.g., "us-east-1")
    /// * `model` - Bedrock model ID (e.g., "anthropic.claude-3-sonnet-20240229-v1:0")
    pub fn new(credentials: BedrockCredentials, region: String, model: String) -> Self {
        Self {
            credentials,
            region,
            model,
        }
    }

    /// Returns the model identifier.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Returns the AWS region.
    pub fn region(&self) -> &str {
        &self.region
    }
}

impl LlmProvider for BedrockProvider {
    fn send_msg(
        &self,
        client: &HttpClient,
        messages: &[Message],
        options: &MessageOptions,
    ) -> Pin<Box<dyn Future<Output = Result<Message, LlmError>> + Send>> {
        // Clone data for the async block
        let client = client.clone();
        let credentials = self.credentials.clone();
        let region = self.region.clone();
        let model = options.model.as_deref().unwrap_or(&self.model).to_string();
        let messages = messages.to_vec();
        let options = options.clone();

        Box::pin(async move {
            // Build request body
            let body = types::build_request_body(&messages, &options)?;

            // Get the API URL for this model
            let url = types::get_converse_url(&region, &model);

            // Sign the request with AWS SigV4
            let headers = signing::sign_request(
                &credentials,
                &region,
                "POST",
                &url,
                &body,
                false, // not streaming
            )?;

            let headers_ref: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();

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
    ) -> StreamMsgFuture {
        // Clone data for the async block
        let client = client.clone();
        let credentials = self.credentials.clone();
        let region = self.region.clone();
        let model = options.model.as_deref().unwrap_or(&self.model).to_string();
        let messages = messages.to_vec();
        let options = options.clone();

        Box::pin(async move {
            // Build request body (same format for streaming)
            let body = types::build_request_body(&messages, &options)?;

            // Get the streaming API URL for this model
            let url = types::get_converse_stream_url(&region, &model);

            // Sign the request with AWS SigV4
            let headers = signing::sign_request(
                &credentials,
                &region,
                "POST",
                &url,
                &body,
                true, // streaming
            )?;

            let headers_ref: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();

            // Make the streaming API call
            let byte_stream = client.post_stream(&url, &headers_ref, &body).await?;

            // Convert byte stream to events
            // Bedrock uses a different format than SSE - it uses event-stream encoding
            use futures::StreamExt;
            let event_stream = stream! {
                let mut buffer = Vec::new();
                let mut byte_stream = byte_stream;
                let mut message_started = false;
                let mut stream_state = sse::StreamState::default();

                while let Some(chunk_result) = byte_stream.next().await {
                    match chunk_result {
                        Ok(bytes) => {
                            buffer.extend_from_slice(&bytes);

                            // Parse complete events from buffer
                            let (events, remaining) = sse::parse_event_stream(&buffer);
                            buffer = remaining;

                            for event in events {
                                match sse::parse_stream_event(&event, &mut stream_state) {
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

            Ok(Box::pin(event_stream)
                as Pin<
                    Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>,
                >)
        })
    }
}
