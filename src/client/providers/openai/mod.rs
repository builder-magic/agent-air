mod types;

use crate::client::error::LlmError;
use crate::client::http::HttpClient;
use crate::client::models::{Message, MessageOptions};
use crate::client::traits::LlmProvider;
use std::future::Future;
use std::pin::Pin;

pub struct OpenAIProvider {
    pub api_key: String,
    pub model: String,
}

impl OpenAIProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
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
}
