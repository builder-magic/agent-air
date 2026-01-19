pub mod error;
pub mod http;
pub mod models;
pub mod providers;
pub mod traits;

use futures::Stream;
use std::pin::Pin;

use crate::error::LlmError;
use crate::http::HttpClient;
use crate::models::{Message, MessageOptions, StreamEvent};
use crate::traits::LlmProvider;

/// This is the main LLM Client.
pub struct VanGogh {
    http_client: HttpClient,
    provider: Box<dyn LlmProvider + Send + Sync>,
}

impl VanGogh {
    pub fn new(provider: Box<dyn LlmProvider + Send + Sync>) -> Self {
        Self {
            http_client: HttpClient::new(),
            provider,
        }
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