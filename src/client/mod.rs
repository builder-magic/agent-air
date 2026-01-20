pub mod error;
pub mod http;
pub mod models;
pub mod providers;
pub mod traits;

use futures::Stream;
use std::pin::Pin;

use error::LlmError;
use http::HttpClient;
use models::{Message, MessageOptions, StreamEvent};
use traits::LlmProvider;

/// This is the main LLM Client.
pub struct LLMClient {
    http_client: HttpClient,
    provider: Box<dyn LlmProvider + Send + Sync>,
}

impl LLMClient {
    pub fn new(provider: Box<dyn LlmProvider + Send + Sync>) -> Result<Self, LlmError> {
        Ok(Self {
            http_client: HttpClient::new()?,
            provider,
        })
    }

    pub async fn send_message(&self, messages: &[Message], options: &MessageOptions) -> Result<Message, LlmError> {
        self.provider.send_msg(&self.http_client, messages, options).await
    }

    pub async fn send_message_stream(
        &self,
        messages: &[Message],
        options: &MessageOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>, LlmError> {
        self.provider.send_msg_stream(&self.http_client, messages, options).await
    }
}