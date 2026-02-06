// This implements a single session with an LLM

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, Ordering};
use std::time::Instant;

use tokio::sync::{Mutex, RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::client::LLMClient;
use crate::client::error::LlmError;
use crate::client::models::Tool as LLMTool;
use crate::client::providers::anthropic::AnthropicProvider;
use crate::client::providers::bedrock::{BedrockCredentials, BedrockProvider};
use crate::client::providers::cohere::CohereProvider;
use crate::client::providers::gemini::GeminiProvider;
use crate::client::providers::openai::OpenAIProvider;

use super::compactor::{AsyncCompactor, Compactor, LLMCompactor, ThresholdCompactor};
use super::config::{CompactorType, LLMProvider, LLMSessionConfig};

/// Creates an LLMClient from the session configuration.
fn create_llm_client(config: &LLMSessionConfig) -> Result<LLMClient, LlmError> {
    match config.provider {
        LLMProvider::Anthropic => {
            let provider = AnthropicProvider::new(config.api_key.clone(), config.model.clone());
            LLMClient::new(Box::new(provider))
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
            LLMClient::new(Box::new(provider))
        }
        LLMProvider::Google => {
            let provider = GeminiProvider::new(config.api_key.clone(), config.model.clone());
            LLMClient::new(Box::new(provider))
        }
        LLMProvider::Cohere => {
            let provider = CohereProvider::new(config.api_key.clone(), config.model.clone());
            LLMClient::new(Box::new(provider))
        }
        LLMProvider::Bedrock => {
            // Bedrock requires all four credential/region fields
            let region = config.bedrock_region.clone().ok_or_else(|| {
                LlmError::new("MISSING_CONFIG", "Bedrock requires bedrock_region")
            })?;
            let access_key_id = config.bedrock_access_key_id.clone().ok_or_else(|| {
                LlmError::new("MISSING_CONFIG", "Bedrock requires bedrock_access_key_id")
            })?;
            let secret_access_key = config.bedrock_secret_access_key.clone().ok_or_else(|| {
                LlmError::new(
                    "MISSING_CONFIG",
                    "Bedrock requires bedrock_secret_access_key",
                )
            })?;

            let credentials = match &config.bedrock_session_token {
                Some(token) => BedrockCredentials::with_session_token(
                    access_key_id,
                    secret_access_key,
                    token.clone(),
                ),
                None => BedrockCredentials::new(access_key_id, secret_access_key),
            };

            let provider = BedrockProvider::new(credentials, region, config.model.clone());
            LLMClient::new(Box::new(provider))
        }
    }
}
use crate::controller::types::{
    AssistantMessage, ContentBlock, FromLLMPayload, Message, ToLLMPayload, TurnId, UserMessage,
};

/// Token usage statistics for the session
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    /// Total input tokens across all requests
    pub total_input_tokens: i64,
    /// Total output tokens across all requests
    pub total_output_tokens: i64,
    /// Number of completed LLM requests
    pub request_count: i64,
    /// Input tokens from the most recent request
    pub last_input_tokens: i64,
    /// Output tokens from the most recent request
    pub last_output_tokens: i64,
}

/// Current status of an LLM session
#[derive(Debug, Clone)]
pub struct SessionStatus {
    /// Session identifier
    pub session_id: i64,
    /// Model name
    pub model: String,
    /// When session was created
    pub created_at: Instant,
    /// Number of messages in conversation
    pub conversation_len: usize,
    /// Current input tokens (context size)
    pub context_used: i64,
    /// Model's context window limit
    pub context_limit: i32,
    /// Context utilization percentage (0-100)
    pub utilization: f64,
    /// Cumulative input tokens
    pub total_input: i64,
    /// Cumulative output tokens
    pub total_output: i64,
    /// Number of API calls
    pub request_count: i64,
}

/// Result of a manual compaction operation.
/// Used by `force_compact()` to report what happened during compaction.
#[derive(Debug, Clone, Default)]
pub struct CompactResult {
    /// Whether compaction was actually performed.
    /// False if there weren't enough turns to compact.
    pub compacted: bool,
    /// Number of messages before compaction.
    pub messages_before: usize,
    /// Number of messages after compaction.
    pub messages_after: usize,
    /// Number of turns that were summarized.
    pub turns_compacted: usize,
    /// Number of recent turns that were preserved.
    pub turns_kept: usize,
    /// Character length of the generated summary (for LLM compaction).
    pub summary_length: usize,
    /// Error message if compaction failed.
    pub error: Option<String>,
}

/// Global counter for generating unique session IDs
static SESSION_COUNTER: AtomicI64 = AtomicI64::new(0);

/// A session that manages communication with an LLM
pub struct LLMSession {
    // Session identification
    id: AtomicI64,

    // LLM client
    client: LLMClient,

    // Channels for communication
    to_llm_tx: mpsc::Sender<ToLLMPayload>,
    to_llm_rx: Mutex<mpsc::Receiver<ToLLMPayload>>,
    from_llm: mpsc::Sender<FromLLMPayload>,

    // Session configuration
    config: LLMSessionConfig,

    // Runtime overrides for LLM options
    system_prompt: RwLock<Option<String>>,
    max_tokens: AtomicI64,
    created_at: Instant,

    // Conversation state
    conversation: RwLock<Arc<Vec<Message>>>,

    // Shutdown management
    shutdown: AtomicBool,
    cancel_token: CancellationToken,

    // Per-request cancellation
    current_cancel: Mutex<Option<CancellationToken>>,

    // Current turn ID for the active request (used for filtering on interrupt)
    current_turn_id: RwLock<Option<TurnId>>,

    // Token tracking for current request
    current_input_tokens: AtomicI64,
    current_output_tokens: AtomicI64,

    // Cumulative token tracking
    request_count: AtomicI64,

    // Tool definitions for LLM API calls
    tool_definitions: RwLock<Vec<LLMTool>>,

    // Compaction support
    compactor: Option<Box<dyn Compactor>>,
    llm_compactor: Option<LLMCompactor>,
    context_limit: AtomicI32,
    compact_summaries: RwLock<HashMap<String, String>>,
}

impl LLMSession {
    /// Creates a new LLM session
    ///
    /// # Arguments
    /// * `config` - Session configuration
    /// * `from_llm` - Sender for outgoing responses
    /// * `cancel_token` - Token for session cancellation
    /// * `channel_size` - Buffer size for the session's input channel
    ///
    /// # Errors
    /// Returns an error if the LLM client fails to initialize (e.g., TLS setup failure)
    pub fn new(
        config: LLMSessionConfig,
        from_llm: mpsc::Sender<FromLLMPayload>,
        cancel_token: CancellationToken,
        channel_size: usize,
    ) -> Result<Self, LlmError> {
        let session_id = SESSION_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
        let (to_llm_tx, to_llm_rx) = mpsc::channel(channel_size);
        let max_tokens = config.max_tokens.unwrap_or(4096) as i64;
        let system_prompt = config.system_prompt.clone();

        // Create the LLMClient client based on the provider
        let client = create_llm_client(&config)?;

        // Create compactor if configured
        let mut compactor: Option<Box<dyn Compactor>> = None;
        let mut llm_compactor: Option<LLMCompactor> = None;

        if let Some(ref compactor_type) = config.compaction {
            match compactor_type {
                CompactorType::Threshold(c) => {
                    match ThresholdCompactor::new(
                        c.threshold,
                        c.keep_recent_turns,
                        c.tool_compaction,
                    ) {
                        Ok(tc) => {
                            tracing::info!(
                                threshold = c.threshold,
                                keep_recent_turns = c.keep_recent_turns,
                                tool_compaction = %c.tool_compaction,
                                "Threshold compaction enabled for session"
                            );
                            compactor = Some(Box::new(tc) as Box<dyn Compactor>);
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to create threshold compactor, compaction disabled");
                        }
                    }
                }
                CompactorType::LLM(c) => {
                    // Create a separate LLMClient client for LLM compaction
                    let llm_client = create_llm_client(&config)?;

                    match LLMCompactor::new(llm_client, c.clone()) {
                        Ok(lc) => {
                            tracing::info!(
                                threshold = c.threshold,
                                keep_recent_turns = c.keep_recent_turns,
                                "LLM compaction enabled for session"
                            );
                            llm_compactor = Some(lc);
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to create LLM compactor, compaction disabled");
                        }
                    }
                }
            }
        }

        let context_limit = config.context_limit;

        Ok(Self {
            id: AtomicI64::new(session_id),
            client,
            to_llm_tx,
            to_llm_rx: Mutex::new(to_llm_rx),
            from_llm,
            config,
            system_prompt: RwLock::new(system_prompt),
            max_tokens: AtomicI64::new(max_tokens),
            created_at: Instant::now(),
            conversation: RwLock::new(Arc::new(Vec::new())),
            shutdown: AtomicBool::new(false),
            cancel_token,
            current_cancel: Mutex::new(None),
            current_turn_id: RwLock::new(None),
            current_input_tokens: AtomicI64::new(0),
            current_output_tokens: AtomicI64::new(0),
            request_count: AtomicI64::new(0),
            tool_definitions: RwLock::new(Vec::new()),
            compactor,
            llm_compactor,
            context_limit: AtomicI32::new(context_limit),
            compact_summaries: RwLock::new(HashMap::new()),
        })
    }

    /// Returns the session ID
    pub fn id(&self) -> i64 {
        self.id.load(Ordering::SeqCst)
    }

    /// Returns when the session was created
    pub fn created_at(&self) -> Instant {
        self.created_at
    }

    /// Returns the model for this session
    pub fn model(&self) -> &str {
        &self.config.model
    }

    // ---- Max Tokens ----

    /// Sets the default maximum tokens for the session
    pub fn set_max_tokens(&self, max_tokens: i64) {
        self.max_tokens.store(max_tokens, Ordering::SeqCst);
    }

    /// Returns the current max tokens setting
    pub fn max_tokens(&self) -> i64 {
        self.max_tokens.load(Ordering::SeqCst)
    }

    /// Returns the context limit for this session's model
    pub fn context_limit(&self) -> i32 {
        self.context_limit.load(Ordering::SeqCst)
    }

    // ---- System Prompt ----

    /// Sets the default system prompt for the session
    pub async fn set_system_prompt(&self, prompt: String) {
        let mut guard = self.system_prompt.write().await;
        *guard = Some(prompt);
    }

    /// Clears the default system prompt
    pub async fn clear_system_prompt(&self) {
        let mut guard = self.system_prompt.write().await;
        *guard = None;
    }

    /// Returns the current system prompt
    pub async fn system_prompt(&self) -> Option<String> {
        self.system_prompt.read().await.clone()
    }

    // ---- Tools ----

    /// Sets the tool definitions for this session.
    /// Tools will be included in all subsequent LLM API calls.
    pub async fn set_tools(&self, tools: Vec<LLMTool>) {
        let mut guard = self.tool_definitions.write().await;
        *guard = tools;
    }

    /// Clears all tool definitions for this session.
    pub async fn clear_tools(&self) {
        let mut guard = self.tool_definitions.write().await;
        guard.clear();
    }

    /// Returns a copy of the current tool definitions.
    pub async fn tools(&self) -> Vec<LLMTool> {
        self.tool_definitions.read().await.clone()
    }

    // ---- Compaction ----

    /// Stores compact summaries for later use during compaction.
    /// Called when tool results are received.
    async fn store_compact_summaries(&self, summaries: &HashMap<String, String>) {
        if summaries.is_empty() {
            tracing::warn!(
                session_id = self.id(),
                "No compact summaries provided with tool results"
            );
            return;
        }
        let mut guard = self.compact_summaries.write().await;
        for (tool_use_id, summary) in summaries {
            tracing::info!(
                session_id = self.id(),
                tool_use_id = %tool_use_id,
                summary_len = summary.len(),
                summary_preview = %summary.chars().take(50).collect::<String>(),
                "Storing compact summary"
            );
            guard.insert(tool_use_id.clone(), summary.clone());
        }
        tracing::info!(
            session_id = self.id(),
            new_summaries = summaries.len(),
            total_stored = guard.len(),
            "Stored compact summaries for tool results"
        );
    }

    /// Performs compaction if needed based on context usage.
    /// Should be called before each LLM request.
    async fn maybe_compact(&self) {
        let context_used = self.current_input_tokens.load(Ordering::SeqCst);
        let context_limit = self.context_limit.load(Ordering::SeqCst);
        let conversation_len = self.conversation.read().await.len();
        let summaries_count = self.compact_summaries.read().await.len();

        let utilization = if context_limit > 0 {
            context_used as f64 / context_limit as f64
        } else {
            0.0
        };

        tracing::debug!(
            session_id = self.id(),
            context_used,
            context_limit,
            utilization = format!("{:.2}%", utilization * 100.0),
            conversation_len,
            summaries_available = summaries_count,
            "Checking if compaction needed"
        );

        // Check for LLM compactor first (async compaction)
        if let Some(ref llm_compactor) = self.llm_compactor {
            if !llm_compactor.should_compact(context_used, context_limit) {
                tracing::debug!(session_id = self.id(), "LLM compaction not triggered");
                return;
            }

            // Get conversation and summaries for async compaction
            let summaries = self.compact_summaries.read().await.clone();
            let conversation_arc = {
                let guard = self.conversation.read().await;
                Arc::clone(&*guard) // O(1)
            };
            let conversation =
                Arc::try_unwrap(conversation_arc).unwrap_or_else(|arc| (*arc).clone());

            tracing::info!(
                session_id = self.id(),
                conversation_len = conversation.len(),
                summaries_count = summaries.len(),
                "Starting LLM compaction"
            );

            // Perform async LLM compaction
            match llm_compactor.compact_async(conversation, &summaries).await {
                Ok((new_conversation, result)) => {
                    // Replace conversation with compacted version
                    *self.conversation.write().await = Arc::new(new_conversation);

                    if result.turns_compacted > 0 {
                        tracing::info!(
                            session_id = self.id(),
                            turns_compacted = result.turns_compacted,
                            "LLM compaction completed"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        session_id = self.id(),
                        error = %e,
                        "LLM compaction failed"
                    );
                }
            }
            return;
        }

        // Fall back to sync compactor (ThresholdCompactor)
        let compactor = match &self.compactor {
            Some(c) => c,
            None => {
                tracing::debug!(session_id = self.id(), "No compactor configured");
                return;
            }
        };

        if !compactor.should_compact(context_used, context_limit) {
            tracing::debug!(session_id = self.id(), "Threshold compaction not triggered");
            return;
        }

        // Perform sync compaction
        let summaries = self.compact_summaries.read().await.clone();
        let mut guard = self.conversation.write().await;

        tracing::info!(
            session_id = self.id(),
            conversation_len = guard.len(),
            summaries_count = summaries.len(),
            "Starting threshold compaction"
        );

        let result = compactor.compact(Arc::make_mut(&mut *guard), &summaries);

        tracing::info!(
            session_id = self.id(),
            tool_results_summarized = result.tool_results_summarized,
            tool_results_redacted = result.tool_results_redacted,
            turns_compacted = result.turns_compacted,
            conversation_len_after = guard.len(),
            "Threshold compaction completed"
        );
    }

    // ---- Session Control ----

    /// Clears the conversation history and compact summaries.
    pub async fn clear_conversation(&self) {
        let mut guard = self.conversation.write().await;
        Arc::make_mut(&mut *guard).clear();

        let mut summaries = self.compact_summaries.write().await;
        summaries.clear();

        // Reset token counters
        self.current_input_tokens.store(0, Ordering::SeqCst);
        self.current_output_tokens.store(0, Ordering::SeqCst);

        tracing::info!(session_id = self.id(), "Conversation cleared");
    }

    /// Forces compaction to run immediately, regardless of threshold.
    /// Returns a `CompactResult` with details about what happened.
    pub async fn force_compact(&self) -> CompactResult {
        // Check for LLM compactor first (async compaction)
        if let Some(ref llm_compactor) = self.llm_compactor {
            let summaries = self.compact_summaries.read().await.clone();
            let conversation_arc = {
                let guard = self.conversation.read().await;
                Arc::clone(&*guard) // O(1)
            };
            let conversation =
                Arc::try_unwrap(conversation_arc).unwrap_or_else(|arc| (*arc).clone());
            let messages_before = conversation.len();
            let turns_before = self.count_unique_turns(&conversation);

            match llm_compactor.compact_async(conversation, &summaries).await {
                Ok((new_conversation, result)) => {
                    let messages_after = new_conversation.len();
                    let turns_after = self.count_unique_turns(&new_conversation);
                    let compacted = messages_after < messages_before;

                    // Extract summary length if compaction happened
                    let summary_length = if compacted && !new_conversation.is_empty() {
                        self.extract_summary_length(&new_conversation[0])
                    } else {
                        0
                    };

                    *self.conversation.write().await = Arc::new(new_conversation);

                    if result.turns_compacted > 0 {
                        tracing::info!(
                            session_id = self.id(),
                            turns_compacted = result.turns_compacted,
                            messages_before,
                            messages_after,
                            "Forced LLM compaction completed"
                        );
                    }

                    return CompactResult {
                        compacted,
                        messages_before,
                        messages_after,
                        turns_compacted: turns_before.saturating_sub(turns_after),
                        turns_kept: turns_after,
                        summary_length,
                        error: None,
                    };
                }
                Err(e) => {
                    tracing::error!(
                        session_id = self.id(),
                        error = %e,
                        "Forced LLM compaction failed"
                    );
                    return CompactResult {
                        compacted: false,
                        messages_before,
                        messages_after: messages_before,
                        turns_compacted: 0,
                        turns_kept: turns_before,
                        summary_length: 0,
                        error: Some(format!("Compaction failed: {}", e)),
                    };
                }
            }
        }

        // Fall back to sync compactor (ThresholdCompactor)
        if let Some(ref compactor) = self.compactor {
            let summaries = self.compact_summaries.read().await.clone();
            let mut guard = self.conversation.write().await;
            let messages_before = guard.len();
            let turns_before = self.count_unique_turns(&guard);

            let result = compactor.compact(Arc::make_mut(&mut *guard), &summaries);

            let messages_after = guard.len();
            let turns_after = self.count_unique_turns(&guard);
            let compacted = result.turns_compacted > 0 || result.total_compacted() > 0;

            if result.total_compacted() > 0 {
                tracing::info!(
                    session_id = self.id(),
                    tool_results_summarized = result.tool_results_summarized,
                    tool_results_redacted = result.tool_results_redacted,
                    turns_compacted = result.turns_compacted,
                    "Forced threshold compaction completed"
                );
            }

            return CompactResult {
                compacted,
                messages_before,
                messages_after,
                turns_compacted: turns_before.saturating_sub(turns_after),
                turns_kept: turns_after,
                summary_length: 0,
                error: None,
            };
        }

        // No compactor configured
        CompactResult {
            compacted: false,
            error: Some("No compactor configured".to_string()),
            ..Default::default()
        }
    }

    /// Count unique turn IDs in a conversation.
    fn count_unique_turns(&self, conversation: &[Message]) -> usize {
        use std::collections::HashSet;
        let mut turn_ids = HashSet::new();
        for msg in conversation {
            turn_ids.insert(msg.turn_id().clone());
        }
        turn_ids.len()
    }

    /// Extract the summary length from a summary message.
    fn extract_summary_length(&self, message: &Message) -> usize {
        if let Message::User(user_msg) = message {
            for block in &user_msg.content {
                if let ContentBlock::Text(text_block) = block
                    && text_block
                        .text
                        .starts_with("[Previous conversation summary]")
                {
                    return text_block.text.len();
                }
            }
        }
        0
    }

    /// Sends a message to the LLM session for processing.
    /// Returns false if the session is shutdown or the channel is closed.
    pub async fn send(&self, msg: ToLLMPayload) -> bool {
        if self.shutdown.load(Ordering::SeqCst) {
            return false;
        }
        self.to_llm_tx.send(msg).await.is_ok()
    }

    /// Interrupts the currently executing LLM request.
    /// This cancels any in-flight request and removes all messages from the
    /// current turn from conversation history. Does not shutdown the session.
    pub async fn interrupt(&self) {
        let guard = self.current_cancel.lock().await;
        if let Some(token) = guard.as_ref() {
            token.cancel();

            // Remove all messages from the current turn from conversation history.
            // This prevents any messages from the cancelled turn (user message,
            // assistant responses, etc.) from being included in subsequent API calls.
            let turn_id = self.current_turn_id.read().await.clone();
            if let Some(turn_id) = turn_id {
                let mut guard = self.conversation.write().await;
                let original_len = guard.len();
                Arc::make_mut(&mut *guard).retain(|msg| msg.turn_id() != &turn_id);
                let removed = original_len - guard.len();
                tracing::debug!(
                    session_id = self.id(),
                    turn_id = %turn_id,
                    messages_removed = removed,
                    conversation_length = guard.len(),
                    "Removed messages from cancelled turn"
                );
            }
        }
    }

    /// Gracefully shuts down the session.
    /// After calling this, the session will not accept new messages.
    pub fn shutdown(&self) {
        // Mark as shutdown to prevent new messages
        self.shutdown.store(true, Ordering::SeqCst);
        // Cancel the session's main loop
        self.cancel_token.cancel();
    }

    /// Returns true if the session has been shutdown
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    // ---- Main Processing Loop ----

    /// Starts the session's main processing loop.
    /// This method processes requests from the ToLLM channel until shutdown.
    /// Should be spawned as a tokio task.
    pub async fn start(&self) {
        tracing::info!(session_id = self.id(), "Session starting");

        loop {
            let mut rx_guard = self.to_llm_rx.lock().await;

            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    tracing::info!(session_id = self.id(), "Session cancelled");
                    break;
                }
                msg = rx_guard.recv() => {
                    match msg {
                        Some(request) => {
                            // Drop the lock before handling the request
                            drop(rx_guard);
                            self.handle_request(request).await;
                        }
                        None => {
                            // Channel closed
                            tracing::info!(session_id = self.id(), "Session channel closed");
                            break;
                        }
                    }
                }
            }
        }

        tracing::info!(session_id = self.id(), "Session stopped");
    }

    // ---- Request Helper Methods ----

    /// Returns the current timestamp in milliseconds.
    fn current_timestamp_millis() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    /// Prepares the request context by setting up cancellation token and turn ID.
    /// Returns the request token and effective turn ID.
    async fn prepare_request(&self, request: &ToLLMPayload) -> (CancellationToken, TurnId) {
        let request_token = CancellationToken::new();
        {
            let mut guard = self.current_cancel.lock().await;
            *guard = Some(request_token.clone());
        }

        let effective_turn_id = request
            .turn_id
            .clone()
            .unwrap_or_else(|| TurnId::new_user_turn(0));
        {
            let mut guard = self.current_turn_id.write().await;
            *guard = Some(effective_turn_id.clone());
        }

        (request_token, effective_turn_id)
    }

    /// Builds the message options with tools for the LLM request.
    async fn build_message_options(&self) -> crate::client::models::MessageOptions {
        use crate::client::models::MessageOptions;

        let max_tokens = self.max_tokens.load(Ordering::SeqCst) as u32;
        let tools = self.tool_definitions.read().await.clone();
        let tools_option = if tools.is_empty() { None } else { Some(tools) };

        MessageOptions {
            max_tokens: Some(max_tokens),
            temperature: self.config.temperature,
            tools: tools_option,
            ..Default::default()
        }
    }

    /// Clears the request cancellation token and turn ID after request completion.
    async fn cleanup_request(&self) {
        {
            let mut guard = self.current_cancel.lock().await;
            *guard = None;
        }
        {
            let mut guard = self.current_turn_id.write().await;
            *guard = None;
        }
    }

    /// Handles a single request from the ToLLM channel.
    async fn handle_request(&self, request: ToLLMPayload) {
        if self.config.streaming {
            self.handle_streaming_request(request).await;
        } else {
            self.handle_non_streaming_request(request).await;
        }
    }

    /// Handles a non-streaming request.
    async fn handle_non_streaming_request(&self, request: ToLLMPayload) {
        use super::convert::{from_llm_message, to_llm_messages};
        use crate::client::models::Message as LLMMessage;
        use crate::controller::types::{LLMRequestType, LLMResponseType};

        // Prepare request context
        let (_request_token, effective_turn_id) = self.prepare_request(&request).await;

        let session_id = self.id();
        tracing::debug!(session_id, turn_id = %effective_turn_id, "Handling request");

        // Build the conversation messages
        let mut llm_messages: Vec<LLMMessage> = Vec::new();

        // Add system prompt if set
        if let Some(prompt) = self.system_prompt.read().await.as_ref() {
            llm_messages.push(LLMMessage::system(prompt.clone()));
        }

        // Add conversation history
        let conversation = self.conversation.read().await;
        llm_messages.extend(to_llm_messages(&conversation));
        drop(conversation);

        // Add the new message based on request type
        match request.request_type {
            LLMRequestType::UserMessage => {
                if !request.content.is_empty() {
                    llm_messages.push(LLMMessage::user(&request.content));

                    // Add user message to conversation history
                    let user_msg = Message::User(UserMessage {
                        id: format!("user_{}", self.request_count.load(Ordering::SeqCst)),
                        session_id: session_id.to_string(),
                        turn_id: effective_turn_id.clone(),
                        created_at: Self::current_timestamp_millis(),
                        content: vec![ContentBlock::text(&request.content)],
                    });
                    Arc::make_mut(&mut *self.conversation.write().await).push(user_msg);
                }
            }
            LLMRequestType::ToolResult => {
                // Store compact summaries for later compaction
                self.store_compact_summaries(&request.compact_summaries)
                    .await;

                // Add tool result messages using LLM client's proper format
                for tool_result in &request.tool_results {
                    llm_messages.push(LLMMessage::tool_result(
                        &tool_result.tool_use_id,
                        &tool_result.content,
                        tool_result.is_error,
                    ));

                    // Get compact summary if available
                    let compact_summary = request
                        .compact_summaries
                        .get(&tool_result.tool_use_id)
                        .cloned();

                    // Add tool result to conversation history
                    let user_msg = Message::User(UserMessage {
                        id: format!("tool_result_{}", self.request_count.load(Ordering::SeqCst)),
                        session_id: session_id.to_string(),
                        turn_id: effective_turn_id.clone(),
                        created_at: Self::current_timestamp_millis(),
                        content: vec![ContentBlock::ToolResult(
                            crate::controller::types::ToolResultBlock {
                                tool_use_id: tool_result.tool_use_id.clone(),
                                content: tool_result.content.clone(),
                                is_error: tool_result.is_error,
                                compact_summary,
                            },
                        )],
                    });
                    Arc::make_mut(&mut *self.conversation.write().await).push(user_msg);
                }
            }
        }

        // Perform compaction if needed before LLM call
        self.maybe_compact().await;

        // Build message options with tools
        let options = self.build_message_options().await;

        // Call the LLM
        let result = self.client.send_message(&llm_messages, &options).await;

        match result {
            Ok(response) => {
                // Convert response to our types
                let content_blocks = from_llm_message(&response);

                // Extract text for the text chunk response
                let text: String = content_blocks
                    .iter()
                    .filter_map(|block| {
                        if let ContentBlock::Text(t) = block {
                            Some(t.text.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");

                // Send text chunk response
                if !text.is_empty() {
                    let payload = FromLLMPayload {
                        session_id,
                        response_type: LLMResponseType::TextChunk,
                        text: text.clone(),
                        turn_id: request.turn_id.clone(),
                        ..Default::default()
                    };
                    let _ = self.from_llm.send(payload).await;
                }

                // Check for tool use
                for block in &content_blocks {
                    if let ContentBlock::ToolUse(tool_use) = block {
                        let payload = FromLLMPayload {
                            session_id,
                            response_type: LLMResponseType::ToolUse,
                            tool_use: Some(crate::controller::types::ToolUseInfo {
                                id: tool_use.id.clone(),
                                name: tool_use.name.clone(),
                                input: serde_json::to_value(&tool_use.input).unwrap_or_default(),
                            }),
                            turn_id: request.turn_id.clone(),
                            ..Default::default()
                        };
                        let _ = self.from_llm.send(payload).await;
                    }
                }

                // Add assistant message to conversation history
                let now = Self::current_timestamp_millis();
                let asst_msg = Message::Assistant(AssistantMessage {
                    id: format!("asst_{}", self.request_count.load(Ordering::SeqCst)),
                    session_id: session_id.to_string(),
                    turn_id: effective_turn_id.clone(),
                    parent_id: String::new(),
                    created_at: now,
                    completed_at: Some(now),
                    model_id: self.config.model.clone(),
                    provider_id: String::new(),
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    finish_reason: None,
                    error: None,
                    content: content_blocks,
                });
                Arc::make_mut(&mut *self.conversation.write().await).push(asst_msg);

                // Send completion
                let payload = FromLLMPayload {
                    session_id,
                    response_type: LLMResponseType::Complete,
                    is_complete: true,
                    turn_id: request.turn_id.clone(),
                    ..Default::default()
                };
                let _ = self.from_llm.send(payload).await;

                // Update request count
                self.request_count.fetch_add(1, Ordering::SeqCst);

                tracing::debug!(session_id, "Request completed successfully");
            }
            Err(err) => {
                tracing::error!(session_id, error = %err, "LLM request failed");

                let payload = FromLLMPayload {
                    session_id,
                    response_type: LLMResponseType::Error,
                    error: Some(err.to_string()),
                    turn_id: request.turn_id,
                    ..Default::default()
                };
                let _ = self.from_llm.send(payload).await;
            }
        }

        // Clear the request cancellation token and turn ID when done
        self.cleanup_request().await;
    }

    /// Handles a streaming request.
    async fn handle_streaming_request(&self, request: ToLLMPayload) {
        use super::convert::to_llm_messages;
        use crate::client::models::{ContentBlockType, Message as LLMMessage, StreamEvent};
        use crate::controller::types::{LLMRequestType, LLMResponseType};
        use futures::StreamExt;

        // Prepare request context
        let (request_token, effective_turn_id) = self.prepare_request(&request).await;

        let session_id = self.id();
        tracing::debug!(session_id, turn_id = %effective_turn_id, "Handling streaming request");

        // Build the conversation messages
        let mut llm_messages: Vec<LLMMessage> = Vec::new();

        // Add system prompt if set
        if let Some(prompt) = self.system_prompt.read().await.as_ref() {
            llm_messages.push(LLMMessage::system(prompt.clone()));
        }

        // Add conversation history
        let conversation = self.conversation.read().await;
        llm_messages.extend(to_llm_messages(&conversation));
        drop(conversation);

        // Add the new message based on request type
        match request.request_type {
            LLMRequestType::UserMessage => {
                if !request.content.is_empty() {
                    llm_messages.push(LLMMessage::user(&request.content));

                    // Add user message to conversation history
                    let user_msg = Message::User(UserMessage {
                        id: format!("user_{}", self.request_count.load(Ordering::SeqCst)),
                        session_id: session_id.to_string(),
                        turn_id: effective_turn_id.clone(),
                        created_at: Self::current_timestamp_millis(),
                        content: vec![ContentBlock::text(&request.content)],
                    });
                    Arc::make_mut(&mut *self.conversation.write().await).push(user_msg);
                }
            }
            LLMRequestType::ToolResult => {
                // Store compact summaries for later compaction
                self.store_compact_summaries(&request.compact_summaries)
                    .await;

                // Log conversation state before adding tool results (streaming path)
                {
                    let conv = self.conversation.read().await;
                    tracing::debug!(
                        session_id,
                        conversation_len = conv.len(),
                        tool_result_count = request.tool_results.len(),
                        "STREAMING ToolResult: conversation state before adding results"
                    );
                }
                // Add tool result messages using LLM client's proper format
                for tool_result in &request.tool_results {
                    llm_messages.push(LLMMessage::tool_result(
                        &tool_result.tool_use_id,
                        &tool_result.content,
                        tool_result.is_error,
                    ));

                    // Get compact summary if available
                    let compact_summary = request
                        .compact_summaries
                        .get(&tool_result.tool_use_id)
                        .cloned();

                    // Add tool result to conversation history
                    let user_msg = Message::User(UserMessage {
                        id: format!("tool_result_{}", self.request_count.load(Ordering::SeqCst)),
                        session_id: session_id.to_string(),
                        turn_id: effective_turn_id.clone(),
                        created_at: Self::current_timestamp_millis(),
                        content: vec![ContentBlock::ToolResult(
                            crate::controller::types::ToolResultBlock {
                                tool_use_id: tool_result.tool_use_id.clone(),
                                content: tool_result.content.clone(),
                                is_error: tool_result.is_error,
                                compact_summary,
                            },
                        )],
                    });
                    Arc::make_mut(&mut *self.conversation.write().await).push(user_msg);
                }
            }
        }

        // Perform compaction if needed before LLM call
        self.maybe_compact().await;

        // Build message options with tools
        let options = self.build_message_options().await;

        // Call the streaming LLM API
        let stream_result = self
            .client
            .send_message_stream(&llm_messages, &options)
            .await;

        match stream_result {
            Ok(mut stream) => {
                // Track current content block for tool use accumulation
                let mut current_tool_id: Option<String> = None;
                let mut current_tool_name: Option<String> = None;
                let mut tool_input_json = String::new();
                // Accumulate response text for conversation history
                let mut response_text = String::new();
                // Accumulate completed tool uses for conversation history
                let mut completed_tool_uses: Vec<crate::controller::types::ToolUseBlock> =
                    Vec::new();

                // Process stream events
                loop {
                    tokio::select! {
                        _ = request_token.cancelled() => {
                            tracing::info!(session_id, "Streaming request cancelled");
                            break;
                        }
                        event = stream.next() => {
                            match event {
                                Some(Ok(stream_event)) => {
                                    match stream_event {
                                        StreamEvent::MessageStart { message_id, model } => {
                                            let payload = FromLLMPayload {
                                                session_id,
                                                response_type: LLMResponseType::StreamStart,
                                                message_id,
                                                model,
                                                turn_id: request.turn_id.clone(),
                                                ..Default::default()
                                            };
                                            let _ = self.from_llm.send(payload).await;
                                        }
                                        StreamEvent::ContentBlockStart { index: _, block_type } => {
                                            match block_type {
                                                ContentBlockType::Text => {
                                                    // Text block starting, nothing special to do
                                                }
                                                ContentBlockType::ToolUse { id, name } => {
                                                    // Track tool use for later batch execution
                                                    // Don't emit events here - wait until execution begins
                                                    current_tool_id = Some(id);
                                                    current_tool_name = Some(name);
                                                    tool_input_json.clear();
                                                }
                                            }
                                        }
                                        StreamEvent::TextDelta { index, text } => {
                                            // Accumulate for conversation history
                                            response_text.push_str(&text);

                                            let payload = FromLLMPayload {
                                                session_id,
                                                response_type: LLMResponseType::TextChunk,
                                                text,
                                                content_index: index,
                                                turn_id: request.turn_id.clone(),
                                                ..Default::default()
                                            };
                                            let _ = self.from_llm.send(payload).await;
                                        }
                                        StreamEvent::InputJsonDelta { index, json } => {
                                            // Accumulate JSON for tool input
                                            tool_input_json.push_str(&json);

                                            let payload = FromLLMPayload {
                                                session_id,
                                                response_type: LLMResponseType::ToolInputDelta,
                                                text: json,
                                                content_index: index,
                                                turn_id: request.turn_id.clone(),
                                                ..Default::default()
                                            };
                                            let _ = self.from_llm.send(payload).await;
                                        }
                                        StreamEvent::ContentBlockStop { index: _ } => {
                                            // If we were accumulating a tool use, save it for later
                                            // Don't emit event - ToolUseStart already emitted at ContentBlockStart
                                            // Wait until MessageStop to emit ToolBatch for execution
                                            if let (Some(id), Some(name)) =
                                                (current_tool_id.take(), current_tool_name.take())
                                            {
                                                let input: serde_json::Value =
                                                    serde_json::from_str(&tool_input_json)
                                                        .unwrap_or(serde_json::Value::Object(
                                                            serde_json::Map::new(),
                                                        ));

                                                // Save tool use for conversation history and batch execution
                                                tracing::debug!(
                                                    session_id,
                                                    tool_id = %id,
                                                    tool_name = %name,
                                                    "Saving tool use to completed_tool_uses"
                                                );
                                                completed_tool_uses.push(crate::controller::types::ToolUseBlock {
                                                    id: id.clone(),
                                                    name: name.clone(),
                                                    input: input
                                                        .as_object()
                                                        .map(|obj| {
                                                            obj.iter()
                                                                .map(|(k, v)| (k.clone(), v.clone()))
                                                                .collect()
                                                        })
                                                        .unwrap_or_default(),
                                                });

                                                tool_input_json.clear();
                                            }
                                        }
                                        StreamEvent::MessageDelta { stop_reason, usage } => {
                                            if let Some(usage) = usage {
                                                tracing::info!(
                                                    session_id,
                                                    input_tokens = usage.input_tokens,
                                                    output_tokens = usage.output_tokens,
                                                    "API token usage for this turn"
                                                );
                                                self.current_input_tokens
                                                    .store(usage.input_tokens as i64, Ordering::SeqCst);
                                                self.current_output_tokens
                                                    .store(usage.output_tokens as i64, Ordering::SeqCst);

                                                let payload = FromLLMPayload {
                                                    session_id,
                                                    response_type: LLMResponseType::TokenUpdate,
                                                    input_tokens: usage.input_tokens as i64,
                                                    output_tokens: usage.output_tokens as i64,
                                                    turn_id: request.turn_id.clone(),
                                                    ..Default::default()
                                                };
                                                let _ = self.from_llm.send(payload).await;
                                            }

                                            if stop_reason.is_some() {
                                                let payload = FromLLMPayload {
                                                    session_id,
                                                    response_type: LLMResponseType::Complete,
                                                    is_complete: true,
                                                    stop_reason,
                                                    turn_id: request.turn_id.clone(),
                                                    ..Default::default()
                                                };
                                                let _ = self.from_llm.send(payload).await;
                                            }
                                        }
                                        StreamEvent::MessageStop => {
                                            // Add assistant message to conversation history
                                            // Must save both text AND tool uses
                                            tracing::debug!(
                                                session_id,
                                                text_len = response_text.len(),
                                                tool_use_count = completed_tool_uses.len(),
                                                "MessageStop: saving assistant message to history"
                                            );
                                            if !response_text.is_empty() || !completed_tool_uses.is_empty() {
                                                let now = Self::current_timestamp_millis();

                                                // Build content blocks: text first, then tool uses
                                                let mut content_blocks = Vec::new();
                                                if !response_text.is_empty() {
                                                    content_blocks.push(ContentBlock::text(&response_text));
                                                }
                                                for tool_use in &completed_tool_uses {
                                                    content_blocks.push(ContentBlock::ToolUse(tool_use.clone()));
                                                }

                                                let content_block_count = content_blocks.len();
                                                let asst_msg = Message::Assistant(AssistantMessage {
                                                    id: format!("asst_{}", self.request_count.load(Ordering::SeqCst)),
                                                    session_id: session_id.to_string(),
                                                    turn_id: effective_turn_id.clone(),
                                                    parent_id: String::new(),
                                                    created_at: now,
                                                    completed_at: Some(now),
                                                    model_id: self.config.model.clone(),
                                                    provider_id: String::new(),
                                                    input_tokens: self.current_input_tokens.load(Ordering::SeqCst),
                                                    output_tokens: self.current_output_tokens.load(Ordering::SeqCst),
                                                    cache_read_tokens: 0,
                                                    cache_write_tokens: 0,
                                                    finish_reason: None,
                                                    error: None,
                                                    content: content_blocks,
                                                });
                                                Arc::make_mut(&mut *self.conversation.write().await).push(asst_msg);
                                                tracing::debug!(
                                                    session_id,
                                                    content_block_count,
                                                    "MessageStop: saved assistant message with content blocks"
                                                );
                                            }

                                            // If there are tool uses, emit them as a batch for execution
                                            // This ensures all tools are executed together and results sent back in one message
                                            if !completed_tool_uses.is_empty() {
                                                let tool_uses: Vec<crate::controller::types::ToolUseInfo> = completed_tool_uses
                                                    .iter()
                                                    .map(|tu| crate::controller::types::ToolUseInfo {
                                                        id: tu.id.clone(),
                                                        name: tu.name.clone(),
                                                        input: serde_json::Value::Object(
                                                            tu.input.iter()
                                                                .map(|(k, v)| (k.clone(), v.clone()))
                                                                .collect()
                                                        ),
                                                    })
                                                    .collect();

                                                tracing::debug!(
                                                    session_id,
                                                    tool_count = tool_uses.len(),
                                                    "MessageStop: emitting ToolBatch for execution"
                                                );

                                                let payload = FromLLMPayload {
                                                    session_id,
                                                    response_type: LLMResponseType::ToolBatch,
                                                    tool_uses,
                                                    turn_id: request.turn_id.clone(),
                                                    ..Default::default()
                                                };
                                                let _ = self.from_llm.send(payload).await;
                                            }

                                            // Stream complete
                                            self.request_count.fetch_add(1, Ordering::SeqCst);
                                            tracing::debug!(session_id, "Streaming request completed");
                                            break;
                                        }
                                        StreamEvent::Ping => {
                                            // Keep-alive, ignore
                                        }
                                    }
                                }
                                Some(Err(err)) => {
                                    tracing::error!(session_id, error = %err, "Stream error");
                                    let payload = FromLLMPayload {
                                        session_id,
                                        response_type: LLMResponseType::Error,
                                        error: Some(err.to_string()),
                                        turn_id: request.turn_id.clone(),
                                        ..Default::default()
                                    };
                                    let _ = self.from_llm.send(payload).await;
                                    break;
                                }
                                None => {
                                    // Stream ended
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            Err(err) => {
                tracing::error!(session_id, error = %err, "Failed to start streaming");
                let payload = FromLLMPayload {
                    session_id,
                    response_type: LLMResponseType::Error,
                    error: Some(err.to_string()),
                    turn_id: request.turn_id,
                    ..Default::default()
                };
                let _ = self.from_llm.send(payload).await;
            }
        }

        // Clear the request cancellation token and turn ID when done
        self.cleanup_request().await;
    }
}
