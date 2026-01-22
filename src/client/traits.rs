use super::error::LlmError;
use super::http::HttpClient;
use super::models::{Message, MessageOptions, StreamEvent};
use futures::Stream;
use std::future::Future;
use std::pin::Pin;

/// Provider interface for LLM APIs.
///
/// Implement this trait to add support for new LLM providers.
pub trait LlmProvider {
    /// Send a message to the LLM.
    /// Returns the assistant's response message or an error.
    fn send_msg(
        &self,
        client: &HttpClient,
        messages: &[Message],
        options: &MessageOptions,
    ) -> Pin<Box<dyn Future<Output = Result<Message, LlmError>> + Send>>;

    /// Send a streaming message to the LLM.
    /// Returns a stream of events as they arrive from the API.
    fn send_msg_stream(
        &self,
        _client: &HttpClient,
        _messages: &[Message],
        _options: &MessageOptions,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>, LlmError>> + Send>> {
        Box::pin(async {
            Err(LlmError::new(
                "NOT_IMPLEMENTED",
                "Streaming not supported for this provider",
            ))
        })
    }
}