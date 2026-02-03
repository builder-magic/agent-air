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

/// Azure OpenAI configuration.
#[derive(Clone)]
pub struct AzureConfig {
    /// Azure resource name (e.g., "my-resource").
    pub resource: String,
    /// Azure deployment name (e.g., "gpt-4-deployment").
    pub deployment: String,
    /// Azure API version (e.g., "2024-10-21").
    pub api_version: String,
}

/// OpenAI API provider.
///
/// Also supports OpenAI-compatible APIs (Groq, Together, Fireworks, etc.)
/// by specifying a custom base_url.
///
/// Also supports Azure OpenAI by specifying an AzureConfig.
pub struct OpenAIProvider {
    /// OpenAI API key.
    api_key: String,
    /// Model identifier (e.g., "gpt-4").
    model: String,
    /// Custom base URL for OpenAI-compatible providers.
    /// If None, uses the default OpenAI endpoint.
    base_url: Option<String>,
    /// Azure configuration. If set, uses Azure OpenAI instead of standard OpenAI.
    azure_config: Option<AzureConfig>,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider with API key and model.
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            base_url: None,
            azure_config: None,
        }
    }

    /// Create a new OpenAI-compatible provider with a custom base URL.
    ///
    /// Use this for providers like Groq, Together, Fireworks, etc.
    /// The base_url should be the API base (e.g., "https://api.groq.com/openai/v1").
    pub fn with_base_url(api_key: String, model: String, base_url: String) -> Self {
        Self {
            api_key,
            model,
            base_url: Some(base_url),
            azure_config: None,
        }
    }

    /// Create a new Azure OpenAI provider.
    ///
    /// Azure OpenAI uses a different URL format and authentication header.
    /// URL: https://{resource}.openai.azure.com/openai/deployments/{deployment}/chat/completions?api-version={version}
    /// Auth: api-key header instead of Authorization: Bearer
    pub fn azure(api_key: String, resource: String, deployment: String, api_version: String) -> Self {
        Self {
            api_key,
            model: String::new(), // Not used for Azure
            base_url: None,
            azure_config: Some(AzureConfig {
                resource,
                deployment,
                api_version,
            }),
        }
    }

    /// Returns the model identifier.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Returns true if this provider is configured for Azure OpenAI.
    pub fn is_azure(&self) -> bool {
        self.azure_config.is_some()
    }

    /// Returns the API endpoint URL.
    fn api_url(&self) -> String {
        if let Some(azure) = &self.azure_config {
            types::get_azure_api_url(&azure.resource, &azure.deployment, &azure.api_version)
        } else {
            types::get_api_url_with_base(self.base_url.as_deref())
        }
    }

    /// Returns the request headers appropriate for this provider configuration.
    fn get_headers(&self) -> Vec<(&'static str, String)> {
        if self.azure_config.is_some() {
            types::get_azure_request_headers(&self.api_key)
        } else {
            types::get_request_headers(&self.api_key)
        }
    }
}

impl LlmProvider for OpenAIProvider {
    fn send_msg(
        &self,
        client: &HttpClient,
        messages: &[Message],
        options: &MessageOptions,
    ) -> Pin<Box<dyn Future<Output = Result<Message, LlmError>> + Send>> {
        // Clone data for the async block
        let client = client.clone();
        let model = self.model.clone();
        let api_url = self.api_url();
        let headers = self.get_headers();
        let messages = messages.to_vec();
        let options = options.clone();

        Box::pin(async move {
            // Build request body
            let body = types::build_request_body(&messages, &options, &model)?;

            // Get headers
            let headers_ref: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .collect();

            // Make the API call
            let response = client.post(&api_url, &headers_ref, &body).await?;

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
        let model = self.model.clone();
        let api_url = self.api_url();
        let headers = self.get_headers();
        let messages = messages.to_vec();
        let options = options.clone();

        Box::pin(async move {
            // Build streaming request body
            let body = types::build_streaming_request_body(&messages, &options, &model)?;

            // Get headers
            let headers_ref: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .collect();

            // Make the streaming API call
            let byte_stream = client.post_stream(&api_url, &headers_ref, &body).await?;

            // Convert byte stream to SSE events stream
            use futures::StreamExt;
            let event_stream = stream! {
                let mut buffer = String::new();
                let mut byte_stream = byte_stream;
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
            };

            Ok(Box::pin(event_stream) as Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>)
        })
    }
}
