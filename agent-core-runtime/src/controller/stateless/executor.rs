use tokio_util::sync::CancellationToken;

use crate::client::models::{Message as LLMMessage, MessageOptions, StreamEvent};
use crate::client::providers::anthropic::AnthropicProvider;
use crate::client::providers::bedrock::{BedrockCredentials, BedrockProvider};
use crate::client::providers::cohere::CohereProvider;
use crate::client::providers::gemini::GeminiProvider;
use crate::client::providers::openai::OpenAIProvider;
use crate::client::LLMClient;

use crate::controller::session::LLMProvider;

use super::types::{
    RequestOptions, StatelessConfig, StatelessError, StatelessResult, StreamCallback,
    DEFAULT_MAX_TOKENS,
};

/// Stateless executor for single LLM requests without session state.
/// Safe for concurrent use - multiple tasks can call execute simultaneously.
pub struct StatelessExecutor {
    client: LLMClient,
    config: StatelessConfig,
}

impl StatelessExecutor {
    /// Creates a new stateless executor with the given configuration.
    pub fn new(config: StatelessConfig) -> Result<Self, StatelessError> {
        config.validate()?;

        let client = match config.provider {
            LLMProvider::Anthropic => {
                let provider =
                    AnthropicProvider::new(config.api_key.clone(), config.model.clone());
                LLMClient::new(Box::new(provider)).map_err(|e| StatelessError::ExecutionFailed {
                    op: "init_client".to_string(),
                    message: format!("failed to initialize LLM client: {}", e),
                })?
            }
            LLMProvider::OpenAI => {
                // Check for Azure configuration first
                let provider = if let (Some(resource), Some(deployment)) =
                    (&config.azure_resource, &config.azure_deployment)
                {
                    let api_version = config
                        .azure_api_version
                        .clone()
                        .unwrap_or_else(|| "2024-10-21".to_string());
                    OpenAIProvider::azure(
                        config.api_key.clone(),
                        resource.clone(),
                        deployment.clone(),
                        api_version,
                    )
                } else if let Some(base_url) = &config.base_url {
                    OpenAIProvider::with_base_url(
                        config.api_key.clone(),
                        config.model.clone(),
                        base_url.clone(),
                    )
                } else {
                    OpenAIProvider::new(config.api_key.clone(), config.model.clone())
                };
                LLMClient::new(Box::new(provider)).map_err(|e| StatelessError::ExecutionFailed {
                    op: "init_client".to_string(),
                    message: format!("failed to initialize LLM client: {}", e),
                })?
            }
            LLMProvider::Google => {
                let provider = GeminiProvider::new(config.api_key.clone(), config.model.clone());
                LLMClient::new(Box::new(provider)).map_err(|e| StatelessError::ExecutionFailed {
                    op: "init_client".to_string(),
                    message: format!("failed to initialize LLM client: {}", e),
                })?
            }
            LLMProvider::Cohere => {
                let provider = CohereProvider::new(config.api_key.clone(), config.model.clone());
                LLMClient::new(Box::new(provider)).map_err(|e| StatelessError::ExecutionFailed {
                    op: "init_client".to_string(),
                    message: format!("failed to initialize LLM client: {}", e),
                })?
            }
            LLMProvider::Bedrock => {
                let region = config.bedrock_region.clone().ok_or_else(|| {
                    StatelessError::ExecutionFailed {
                        op: "init_client".to_string(),
                        message: "Bedrock requires bedrock_region".to_string(),
                    }
                })?;
                let access_key_id = config.bedrock_access_key_id.clone().ok_or_else(|| {
                    StatelessError::ExecutionFailed {
                        op: "init_client".to_string(),
                        message: "Bedrock requires bedrock_access_key_id".to_string(),
                    }
                })?;
                let secret_access_key = config.bedrock_secret_access_key.clone().ok_or_else(|| {
                    StatelessError::ExecutionFailed {
                        op: "init_client".to_string(),
                        message: "Bedrock requires bedrock_secret_access_key".to_string(),
                    }
                })?;

                let credentials = match &config.bedrock_session_token {
                    Some(token) => {
                        BedrockCredentials::with_session_token(access_key_id, secret_access_key, token.clone())
                    }
                    None => BedrockCredentials::new(access_key_id, secret_access_key),
                };

                let provider = BedrockProvider::new(credentials, region, config.model.clone());
                LLMClient::new(Box::new(provider)).map_err(|e| StatelessError::ExecutionFailed {
                    op: "init_client".to_string(),
                    message: format!("failed to initialize LLM client: {}", e),
                })?
            }
        };

        Ok(Self { client, config })
    }

    /// Sends a single request to the LLM and waits for the complete response.
    /// This is the simplest API - use execute_stream for progress feedback.
    pub async fn execute(
        &self,
        input: &str,
        options: Option<RequestOptions>,
    ) -> Result<StatelessResult, StatelessError> {
        if input.is_empty() {
            return Err(StatelessError::EmptyInput);
        }

        let msg_opts = self.build_message_options(options.as_ref());
        let mut messages = Vec::new();

        // Add system prompt if configured
        let system_prompt = options
            .as_ref()
            .and_then(|o| o.system_prompt.as_ref())
            .or(self.config.system_prompt.as_ref());

        if let Some(prompt) = system_prompt {
            messages.push(LLMMessage::system(prompt));
        }

        // Add user message
        messages.push(LLMMessage::user(input));

        // Send request
        let response = self
            .client
            .send_message(&messages, &msg_opts)
            .await
            .map_err(|e| StatelessError::ExecutionFailed {
                op: "send_message".to_string(),
                message: e.to_string(),
            })?;

        // Extract text from response
        let text = self.extract_text(&response);

        Ok(StatelessResult {
            text,
            input_tokens: 0,  // Non-streaming doesn't provide usage
            output_tokens: 0, // Non-streaming doesn't provide usage
            model: self.config.model.clone(),
            stop_reason: None,
        })
    }

    /// Sends a request and streams the response via callback.
    /// The callback is called for each text chunk as it arrives.
    /// Returns the complete Result after streaming finishes.
    pub async fn execute_stream(
        &self,
        input: &str,
        mut callback: StreamCallback,
        options: Option<RequestOptions>,
        cancel_token: Option<CancellationToken>,
    ) -> Result<StatelessResult, StatelessError> {
        use futures::StreamExt;

        if input.is_empty() {
            return Err(StatelessError::EmptyInput);
        }

        let msg_opts = self.build_message_options(options.as_ref());
        let mut messages = Vec::new();

        // Add system prompt if configured
        let system_prompt = options
            .as_ref()
            .and_then(|o| o.system_prompt.as_ref())
            .or(self.config.system_prompt.as_ref());

        if let Some(prompt) = system_prompt {
            messages.push(LLMMessage::system(prompt));
        }

        // Add user message
        messages.push(LLMMessage::user(input));

        // Create streaming request
        let mut stream = self
            .client
            .send_message_stream(&messages, &msg_opts)
            .await
            .map_err(|e| StatelessError::ExecutionFailed {
                op: "create_stream".to_string(),
                message: e.to_string(),
            })?;

        // Process stream events
        let mut result = StatelessResult {
            model: self.config.model.clone(),
            ..Default::default()
        };
        let mut text_builder = String::new();
        let cancel = cancel_token.unwrap_or_else(CancellationToken::new);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(StatelessError::Cancelled);
                }
                event = stream.next() => {
                    match event {
                        Some(Ok(stream_event)) => {
                            match stream_event {
                                StreamEvent::MessageStart { model, .. } => {
                                    result.model = model;
                                }
                                StreamEvent::TextDelta { text, .. } => {
                                    text_builder.push_str(&text);
                                    // Call the callback
                                    if callback(&text).is_err() {
                                        return Err(StatelessError::StreamInterrupted);
                                    }
                                }
                                StreamEvent::MessageDelta { stop_reason, usage } => {
                                    if let Some(usage) = usage {
                                        result.input_tokens = usage.input_tokens as i64;
                                        result.output_tokens = usage.output_tokens as i64;
                                    }
                                    result.stop_reason = stop_reason;
                                }
                                StreamEvent::MessageStop => {
                                    break;
                                }
                                // Ignore other events (tool use, etc.)
                                _ => {}
                            }
                        }
                        Some(Err(e)) => {
                            return Err(StatelessError::ExecutionFailed {
                                op: "streaming".to_string(),
                                message: e.to_string(),
                            });
                        }
                        None => {
                            // Stream ended
                            break;
                        }
                    }
                }
            }
        }

        result.text = text_builder;
        Ok(result)
    }

    /// Builds MessageOptions from config and request options.
    fn build_message_options(&self, opts: Option<&RequestOptions>) -> MessageOptions {
        let max_tokens = opts
            .and_then(|o| o.max_tokens)
            .unwrap_or(if self.config.max_tokens > 0 {
                self.config.max_tokens
            } else {
                DEFAULT_MAX_TOKENS
            });

        let temperature = opts
            .and_then(|o| o.temperature)
            .or(self.config.temperature);

        MessageOptions {
            max_tokens: Some(max_tokens),
            temperature,
            ..Default::default()
        }
    }

    /// Extracts text from a LLMClient message response.
    fn extract_text(&self, message: &LLMMessage) -> String {
        use crate::client::models::Content;

        let mut text = String::new();
        for block in &message.content {
            if let Content::Text(t) = block {
                text.push_str(&t);
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        // Missing API key
        let config = StatelessConfig {
            provider: LLMProvider::Anthropic,
            api_key: "".to_string(),
            model: "claude-3".to_string(),
            base_url: None,
            max_tokens: 4096,
            system_prompt: None,
            temperature: None,
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        };
        assert!(config.validate().is_err());

        // Missing model
        let config = StatelessConfig {
            provider: LLMProvider::Anthropic,
            api_key: "test-key".to_string(),
            model: "".to_string(),
            base_url: None,
            max_tokens: 4096,
            system_prompt: None,
            temperature: None,
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        };
        assert!(config.validate().is_err());

        // Valid config
        let config = StatelessConfig::anthropic("test-key", "claude-3");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_request_options_builder() {
        let opts = RequestOptions::new()
            .with_model("gpt-4")
            .with_max_tokens(2048)
            .with_system_prompt("Be helpful")
            .with_temperature(0.7);

        assert_eq!(opts.model, Some("gpt-4".to_string()));
        assert_eq!(opts.max_tokens, Some(2048));
        assert_eq!(opts.system_prompt, Some("Be helpful".to_string()));
        assert_eq!(opts.temperature, Some(0.7));
    }

    #[test]
    fn test_config_builder() {
        let config = StatelessConfig::anthropic("key", "model")
            .with_max_tokens(8192)
            .with_system_prompt("You are helpful")
            .with_temperature(0.5);

        assert_eq!(config.api_key, "key");
        assert_eq!(config.model, "model");
        assert_eq!(config.max_tokens, 8192);
        assert_eq!(config.system_prompt, Some("You are helpful".to_string()));
        assert_eq!(config.temperature, Some(0.5));
    }
}
