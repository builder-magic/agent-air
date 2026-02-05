use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use thiserror::Error;

use crate::client::LLMClient;
use crate::client::models::{Message as LLMMessage, MessageOptions};

use crate::controller::types::{ContentBlock, Message, TextBlock, TurnId, UserMessage};

/// Error type for compactor configuration.
#[derive(Error, Debug)]
pub enum CompactorConfigError {
    /// Invalid threshold value.
    #[error("Invalid threshold {0}: must be between 0.0 and 1.0 (exclusive)")]
    InvalidThreshold(f64),

    /// Invalid keep_recent_turns value.
    #[error("Invalid keep_recent_turns {0}: must be at least 1")]
    InvalidKeepRecentTurns(usize),
}

/// Defines how tool results are handled during compaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCompaction {
    /// Replace tool results with summaries (uses CompactSummary if available).
    Summarize,
    /// Replace tool result content with a redaction notice.
    Redact,
}

impl std::fmt::Display for ToolCompaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolCompaction::Summarize => write!(f, "summarize"),
            ToolCompaction::Redact => write!(f, "redact"),
        }
    }
}

/// Result of a compaction operation.
#[derive(Debug, Clone, Default)]
pub struct CompactionResult {
    /// Number of tool results that were summarized.
    pub tool_results_summarized: usize,
    /// Number of tool results that were redacted.
    pub tool_results_redacted: usize,
    /// Number of turns that were compacted.
    pub turns_compacted: usize,
}

impl CompactionResult {
    /// Total number of tool results compacted.
    pub fn total_compacted(&self) -> usize {
        self.tool_results_summarized + self.tool_results_redacted
    }
}

/// Error type for async compaction operations.
#[derive(Debug)]
pub enum CompactionError {
    /// LLM call failed during summarization.
    LLMError(String),
    /// Timeout during summarization.
    Timeout,
    /// Configuration error.
    ConfigError(String),
}

impl std::fmt::Display for CompactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompactionError::LLMError(msg) => write!(f, "LLM error: {}", msg),
            CompactionError::Timeout => write!(f, "compaction timed out"),
            CompactionError::ConfigError(msg) => write!(f, "config error: {}", msg),
        }
    }
}

impl std::error::Error for CompactionError {}

/// Trait for conversation compaction strategies.
/// Implementations decide when and how to compact conversation history
/// to reduce token usage while preserving important context.
pub trait Compactor: Send + Sync {
    /// Returns true if compaction should occur before this LLM call.
    ///
    /// # Arguments
    /// * `context_used` - Current number of tokens in the conversation context
    /// * `context_limit` - Maximum context size for the model
    fn should_compact(&self, context_used: i64, context_limit: i32) -> bool;

    /// Performs synchronous compaction on the conversation.
    /// Returns the compaction statistics.
    ///
    /// # Arguments
    /// * `conversation` - The conversation messages to compact
    /// * `compact_summaries` - Map of tool_use_id to pre-computed compact summaries
    fn compact(
        &self,
        conversation: &mut Vec<Message>,
        compact_summaries: &HashMap<String, String>,
    ) -> CompactionResult;

    /// Returns true if this compactor requires async compaction.
    /// When true, the caller should use `AsyncCompactor::compact_async()` instead of `compact()`.
    fn is_async(&self) -> bool {
        false
    }
}

/// Trait for async compaction strategies that require LLM calls.
/// Implementors must also implement `Compactor` with `is_async() -> true`.
pub trait AsyncCompactor: Compactor {
    /// Performs async compaction that may involve LLM calls.
    /// Returns the new conversation and compaction statistics.
    ///
    /// Unlike sync `compact()` which modifies in place, this returns a new conversation
    /// because async compaction may replace multiple messages with a single summary.
    fn compact_async<'a>(
        &'a self,
        conversation: Vec<Message>,
        compact_summaries: &'a HashMap<String, String>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<(Vec<Message>, CompactionResult), CompactionError>>
                + Send
                + 'a,
        >,
    >;
}

/// Compacts when context usage exceeds a threshold.
/// It identifies turns to keep (most recent N) and applies the configured
/// compaction strategy to older tool results.
pub struct ThresholdCompactor {
    /// Context utilization ratio (0.0-1.0) that triggers compaction.
    /// For example, 0.75 means compact when 75% of context is used.
    threshold: f64,

    /// Number of recent turns to preserve during compaction.
    /// A turn includes user message, assistant response, tool calls, and tool results.
    keep_recent_turns: usize,

    /// Strategy for handling old tool results.
    tool_compaction: ToolCompaction,
}

impl ThresholdCompactor {
    /// Creates a new threshold compactor.
    ///
    /// # Arguments
    /// * `threshold` - Context utilization ratio (0.0-1.0, exclusive)
    /// * `keep_recent_turns` - Number of recent turns to preserve
    /// * `tool_compaction` - Strategy for handling old tool results
    ///
    /// # Returns
    /// Error if parameters are invalid.
    pub fn new(
        threshold: f64,
        keep_recent_turns: usize,
        tool_compaction: ToolCompaction,
    ) -> Result<Self, CompactorConfigError> {
        if threshold <= 0.0 || threshold >= 1.0 {
            return Err(CompactorConfigError::InvalidThreshold(threshold));
        }

        Ok(Self {
            threshold,
            keep_recent_turns,
            tool_compaction,
        })
    }

    /// Extract unique turn IDs in order of first appearance.
    fn unique_turn_ids(&self, conversation: &[Message]) -> Vec<TurnId> {
        let mut seen = HashSet::new();
        let mut ids = Vec::new();

        for msg in conversation {
            let turn_id = msg.turn_id();
            if seen.insert(turn_id.clone()) {
                ids.push(turn_id.clone());
            }
        }

        ids
    }

    /// Compact tool results in a single message.
    fn compact_message(
        &self,
        msg: &mut Message,
        compact_summaries: &HashMap<String, String>,
    ) -> (usize, usize) {
        let mut summarized = 0;
        let mut redacted = 0;

        for block in msg.content_mut() {
            if let ContentBlock::ToolResult(tool_result) = block {
                match self.tool_compaction {
                    ToolCompaction::Summarize => {
                        // Use pre-computed summary if available
                        if let Some(summary) = compact_summaries.get(&tool_result.tool_use_id) {
                            tool_result.content = summary.clone();
                            summarized += 1;
                            tracing::debug!(
                                tool_use_id = %tool_result.tool_use_id,
                                "Tool result summarized"
                            );
                        }
                        // If no summary available, keep original content
                    }
                    ToolCompaction::Redact => {
                        tool_result.content =
                            "[Tool result redacted during compaction]".to_string();
                        redacted += 1;
                        tracing::debug!(
                            tool_use_id = %tool_result.tool_use_id,
                            "Tool result redacted"
                        );
                    }
                }
            }
        }

        (summarized, redacted)
    }
}

impl Compactor for ThresholdCompactor {
    fn should_compact(&self, context_used: i64, context_limit: i32) -> bool {
        if context_limit == 0 {
            return false;
        }

        let utilization = context_used as f64 / context_limit as f64;
        let should_compact = utilization > self.threshold;

        if should_compact {
            tracing::info!(
                utilization = utilization,
                threshold = self.threshold,
                context_used,
                context_limit,
                "Compaction triggered - context utilization exceeded threshold"
            );
        }

        should_compact
    }

    fn compact(
        &self,
        conversation: &mut Vec<Message>,
        compact_summaries: &HashMap<String, String>,
    ) -> CompactionResult {
        if conversation.is_empty() {
            return CompactionResult::default();
        }

        // Find unique turn IDs in order
        let turn_ids = self.unique_turn_ids(conversation);

        // If fewer turns than keep_recent_turns, nothing to compact
        if turn_ids.len() <= self.keep_recent_turns {
            tracing::debug!(
                total_turns = turn_ids.len(),
                keep_recent = self.keep_recent_turns,
                "Skipping compaction - not enough turns"
            );
            return CompactionResult::default();
        }

        // Identify turns to keep (most recent N)
        let start_idx = turn_ids.len() - self.keep_recent_turns;
        let turns_to_keep: HashSet<_> = turn_ids[start_idx..].iter().cloned().collect();
        let turns_compacted = start_idx;

        tracing::info!(
            total_turns = turn_ids.len(),
            keep_recent = self.keep_recent_turns,
            compacting_turns = turns_compacted,
            tool_compaction_strategy = %self.tool_compaction,
            "Starting conversation compaction"
        );

        // Compact older messages
        let mut total_summarized = 0;
        let mut total_redacted = 0;

        for msg in conversation.iter_mut() {
            let turn_id = msg.turn_id();
            if turns_to_keep.contains(turn_id) {
                continue; // Keep recent turns intact
            }

            let (summarized, redacted) = self.compact_message(msg, compact_summaries);
            total_summarized += summarized;
            total_redacted += redacted;
        }

        tracing::info!(
            tool_results_summarized = total_summarized,
            tool_results_redacted = total_redacted,
            turns_compacted,
            "Conversation compaction completed"
        );

        CompactionResult {
            tool_results_summarized: total_summarized,
            tool_results_redacted: total_redacted,
            turns_compacted,
        }
    }
}

// ============================================================================
// LLM-Based Compaction
// ============================================================================

/// Default system prompt for conversation summarization.
pub const DEFAULT_SUMMARY_SYSTEM_PROMPT: &str = r#"You are a conversation summarizer. Your task is to create a concise summary of the conversation history provided.

Guidelines:
- Capture the key topics discussed, decisions made, and important context
- Preserve any technical details, file paths, code snippets, or specific values that would be needed to continue the conversation
- Include the user's original goals and any progress made toward them
- Note any pending tasks or unresolved questions
- Keep the summary focused and actionable
- Format the summary as a narrative that provides context for continuing the conversation

Respond with only the summary, no additional commentary."#;

/// Default maximum tokens for summary responses.
pub const DEFAULT_MAX_SUMMARY_TOKENS: i64 = 2048;

/// Default timeout for summarization LLM calls (60 seconds).
pub const DEFAULT_SUMMARY_TIMEOUT: Duration = Duration::from_secs(60);

/// Configuration for LLM-based conversation compaction.
#[derive(Debug, Clone)]
pub struct LLMCompactorConfig {
    /// Context utilization ratio (0.0-1.0) that triggers compaction.
    pub threshold: f64,

    /// Number of recent turns to preserve during compaction.
    pub keep_recent_turns: usize,

    /// System prompt for summarization. Uses DEFAULT_SUMMARY_SYSTEM_PROMPT if None.
    pub summary_system_prompt: Option<String>,

    /// Maximum tokens for summary response. Uses DEFAULT_MAX_SUMMARY_TOKENS if None.
    pub max_summary_tokens: Option<i64>,

    /// Timeout for summarization call. Uses DEFAULT_SUMMARY_TIMEOUT if None.
    pub summary_timeout: Option<Duration>,
}

impl LLMCompactorConfig {
    /// Creates a new LLM compactor config with default optional values.
    pub fn new(threshold: f64, keep_recent_turns: usize) -> Self {
        Self {
            threshold,
            keep_recent_turns,
            summary_system_prompt: None,
            max_summary_tokens: None,
            summary_timeout: None,
        }
    }

    /// Validates the configuration.
    pub fn validate(&self) -> Result<(), CompactorConfigError> {
        if self.threshold <= 0.0 || self.threshold >= 1.0 {
            return Err(CompactorConfigError::InvalidThreshold(self.threshold));
        }
        Ok(())
    }

    /// Returns the system prompt to use (config value or default).
    pub fn system_prompt(&self) -> &str {
        self.summary_system_prompt
            .as_deref()
            .unwrap_or(DEFAULT_SUMMARY_SYSTEM_PROMPT)
    }

    /// Returns the max tokens to use (config value or default).
    pub fn max_tokens(&self) -> i64 {
        self.max_summary_tokens
            .unwrap_or(DEFAULT_MAX_SUMMARY_TOKENS)
    }

    /// Returns the timeout to use (config value or default).
    pub fn timeout(&self) -> Duration {
        self.summary_timeout.unwrap_or(DEFAULT_SUMMARY_TIMEOUT)
    }
}

impl Default for LLMCompactorConfig {
    fn default() -> Self {
        Self::new(0.75, 5)
    }
}

/// LLM-based compactor that summarizes older conversation using an LLM.
/// It replaces older messages with a single summary message while preserving recent turns.
pub struct LLMCompactor {
    /// LLMClient client for making summarization LLM calls.
    client: LLMClient,

    /// Configuration for compaction behavior.
    config: LLMCompactorConfig,
}

impl LLMCompactor {
    /// Creates a new LLM compactor.
    ///
    /// # Arguments
    /// * `client` - LLMClient client for making LLM calls
    /// * `config` - Configuration for compaction behavior
    ///
    /// # Returns
    /// Error if configuration is invalid.
    pub fn new(
        client: LLMClient,
        config: LLMCompactorConfig,
    ) -> Result<Self, CompactorConfigError> {
        config.validate()?;

        tracing::info!(
            threshold = config.threshold,
            keep_recent_turns = config.keep_recent_turns,
            max_summary_tokens = config.max_tokens(),
            "LLM compactor created"
        );

        Ok(Self { client, config })
    }

    /// Extract unique turn IDs in order of first appearance.
    fn unique_turn_ids(&self, conversation: &[Message]) -> Vec<TurnId> {
        let mut seen = HashSet::new();
        let mut ids = Vec::new();

        for msg in conversation {
            let turn_id = msg.turn_id();
            if seen.insert(turn_id.clone()) {
                ids.push(turn_id.clone());
            }
        }

        ids
    }

    /// Format messages for LLM summarization.
    fn format_messages_for_summary(&self, messages: &[Message]) -> String {
        let mut builder = String::new();

        for msg in messages {
            // Add role label
            if msg.is_user() {
                builder.push_str("User: ");
            } else {
                builder.push_str("Assistant: ");
            }

            // Format content blocks
            for block in msg.content() {
                match block {
                    ContentBlock::Text(text) => {
                        builder.push_str(&text.text);
                    }
                    ContentBlock::ToolUse(tool_use) => {
                        builder.push_str(&format!(
                            "[Called tool: {} with input: {:?}]",
                            tool_use.name, tool_use.input
                        ));
                    }
                    ContentBlock::ToolResult(tool_result) => {
                        let content = truncate_content(&tool_result.content, 1000);
                        if tool_result.is_error {
                            builder.push_str(&format!("[Tool error: {}]", content));
                        } else {
                            builder.push_str(&format!("[Tool result: {}]", content));
                        }
                    }
                }
            }
            builder.push_str("\n\n");
        }

        builder
    }

    /// Create a summary message from LLM output.
    fn create_summary_message(&self, summary: &str, session_id: &str) -> Message {
        // Use turn number 0 to ensure it comes before all other turns
        let turn_id = TurnId::new_user_turn(0);

        // Use std::time for timestamps
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();

        Message::User(UserMessage {
            id: format!("summary-{}", now.as_nanos()),
            session_id: session_id.to_string(),
            turn_id,
            created_at: now.as_secs() as i64,
            content: vec![ContentBlock::Text(TextBlock {
                text: format!("[Previous conversation summary]:\n\n{}", summary),
            })],
        })
    }

    /// Get session ID from conversation (uses first message's session_id).
    fn get_session_id(&self, conversation: &[Message]) -> String {
        conversation
            .first()
            .map(|m| m.session_id().to_string())
            .unwrap_or_default()
    }
}

impl Compactor for LLMCompactor {
    fn should_compact(&self, context_used: i64, context_limit: i32) -> bool {
        if context_limit == 0 {
            return false;
        }

        let utilization = context_used as f64 / context_limit as f64;
        let should_compact = utilization > self.config.threshold;

        if should_compact {
            tracing::info!(
                utilization = utilization,
                threshold = self.config.threshold,
                context_used,
                context_limit,
                "LLM compaction triggered - context utilization exceeded threshold"
            );
        }

        should_compact
    }

    fn compact(
        &self,
        _conversation: &mut Vec<Message>,
        _compact_summaries: &HashMap<String, String>,
    ) -> CompactionResult {
        // LLMCompactor requires async - this should not be called
        tracing::warn!("LLMCompactor::compact() called - use compact_async() instead");
        CompactionResult::default()
    }

    fn is_async(&self) -> bool {
        true
    }
}

impl AsyncCompactor for LLMCompactor {
    fn compact_async<'a>(
        &'a self,
        conversation: Vec<Message>,
        _compact_summaries: &'a HashMap<String, String>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<(Vec<Message>, CompactionResult), CompactionError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            if conversation.is_empty() {
                return Ok((conversation, CompactionResult::default()));
            }

            // Find unique turn IDs in order
            let turn_ids = self.unique_turn_ids(&conversation);

            // If fewer turns than keep_recent_turns, nothing to compact
            if turn_ids.len() <= self.config.keep_recent_turns {
                tracing::debug!(
                    total_turns = turn_ids.len(),
                    keep_recent = self.config.keep_recent_turns,
                    "Skipping LLM compaction - not enough turns"
                );
                return Ok((conversation, CompactionResult::default()));
            }

            // Separate messages into old (to summarize) and recent (to keep)
            let start_idx = turn_ids.len() - self.config.keep_recent_turns;
            let turns_to_keep: HashSet<_> = turn_ids[start_idx..].iter().cloned().collect();

            let mut old_messages = Vec::new();
            let mut recent_messages = Vec::new();

            for msg in conversation {
                if turns_to_keep.contains(msg.turn_id()) {
                    recent_messages.push(msg);
                } else {
                    old_messages.push(msg);
                }
            }

            if old_messages.is_empty() {
                return Ok((recent_messages, CompactionResult::default()));
            }

            let session_id = self.get_session_id(&old_messages);

            // Format old messages for summarization
            let formatted_conversation = self.format_messages_for_summary(&old_messages);

            tracing::info!(
                total_turns = turn_ids.len(),
                turns_to_summarize = start_idx,
                turns_to_keep = self.config.keep_recent_turns,
                messages_to_summarize = old_messages.len(),
                formatted_length = formatted_conversation.len(),
                "Starting LLM conversation compaction"
            );

            // Call LLM for summarization with timeout
            let options = MessageOptions {
                max_tokens: Some(self.config.max_tokens() as u32),
                ..Default::default()
            };

            // Create messages for summarization
            let llm_messages = vec![
                LLMMessage::system(self.config.system_prompt()),
                LLMMessage::user(formatted_conversation),
            ];

            // Make LLM call with timeout
            let result = tokio::time::timeout(
                self.config.timeout(),
                self.client.send_message(&llm_messages, &options),
            )
            .await;

            let response = match result {
                Ok(Ok(msg)) => msg,
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "LLM compaction failed");
                    return Err(CompactionError::LLMError(e.to_string()));
                }
                Err(_) => {
                    tracing::error!("LLM compaction timed out");
                    return Err(CompactionError::Timeout);
                }
            };

            // Extract text from response
            let summary_text = response
                .content
                .iter()
                .filter_map(|c| {
                    if let crate::client::models::Content::Text(t) = c {
                        Some(t.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("");

            // Create summary message
            let summary_message = self.create_summary_message(&summary_text, &session_id);

            // Build new conversation: summary + recent messages
            let mut new_conversation = Vec::with_capacity(1 + recent_messages.len());
            new_conversation.push(summary_message);
            new_conversation.extend(recent_messages);

            let result = CompactionResult {
                tool_results_summarized: 0,
                tool_results_redacted: 0,
                turns_compacted: start_idx,
            };

            tracing::info!(
                original_messages = old_messages.len() + result.turns_compacted,
                new_messages = new_conversation.len(),
                summary_length = summary_text.len(),
                turns_compacted = result.turns_compacted,
                "LLM compaction completed"
            );

            Ok((new_conversation, result))
        })
    }
}

/// Truncate content to a maximum length, adding ellipsis if needed.
fn truncate_content(content: &str, max_len: usize) -> String {
    if content.len() <= max_len {
        content.to_string()
    } else {
        format!("{}...", &content[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::types::{AssistantMessage, UserMessage};

    fn make_user_message(turn_id: TurnId) -> Message {
        Message::User(UserMessage {
            id: format!("msg_{}", turn_id),
            session_id: "test_session".to_string(),
            turn_id,
            created_at: 0,
            content: vec![ContentBlock::Text(crate::controller::types::TextBlock {
                text: "test".to_string(),
            })],
        })
    }

    fn make_assistant_message(turn_id: TurnId) -> Message {
        Message::Assistant(AssistantMessage {
            id: format!("msg_{}", turn_id),
            session_id: "test_session".to_string(),
            turn_id,
            parent_id: String::new(),
            created_at: 0,
            completed_at: None,
            model_id: "test_model".to_string(),
            provider_id: "test_provider".to_string(),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            finish_reason: None,
            error: None,
            content: vec![ContentBlock::Text(crate::controller::types::TextBlock {
                text: "test".to_string(),
            })],
        })
    }

    fn make_tool_result_message(tool_use_id: &str, content: &str, turn_id: TurnId) -> Message {
        Message::User(UserMessage {
            id: format!("msg_{}", turn_id),
            session_id: "test_session".to_string(),
            turn_id,
            created_at: 0,
            content: vec![ContentBlock::ToolResult(
                crate::controller::types::ToolResultBlock {
                    tool_use_id: tool_use_id.to_string(),
                    content: content.to_string(),
                    is_error: false,
                    compact_summary: None,
                },
            )],
        })
    }

    #[test]
    fn test_threshold_compactor_creation() {
        // Valid threshold
        let compactor = ThresholdCompactor::new(0.75, 3, ToolCompaction::Redact);
        assert!(compactor.is_ok());

        // Invalid threshold (too low)
        let compactor = ThresholdCompactor::new(0.0, 3, ToolCompaction::Redact);
        assert!(compactor.is_err());

        // Invalid threshold (too high)
        let compactor = ThresholdCompactor::new(1.0, 3, ToolCompaction::Redact);
        assert!(compactor.is_err());
    }

    #[test]
    fn test_should_compact() {
        let compactor = ThresholdCompactor::new(0.75, 3, ToolCompaction::Redact).unwrap();

        // Below threshold - don't compact
        assert!(!compactor.should_compact(7000, 10000));

        // Above threshold - compact
        assert!(compactor.should_compact(8000, 10000));

        // Zero context limit - don't compact
        assert!(!compactor.should_compact(8000, 0));
    }

    #[test]
    fn test_compact_not_enough_turns() {
        let compactor = ThresholdCompactor::new(0.75, 3, ToolCompaction::Redact).unwrap();

        let mut conversation = vec![
            make_user_message(TurnId::new_user_turn(1)),
            make_assistant_message(TurnId::new_assistant_turn(1)),
        ];

        let summaries = std::collections::HashMap::new();
        let result = compactor.compact(&mut conversation, &summaries);

        // Only 2 turns, but we keep 3, so nothing compacted
        assert_eq!(result.turns_compacted, 0);
    }

    #[test]
    fn test_compact_redacts_old_tool_results() {
        // keep_recent_turns=2 keeps u2 and a2, compacts u1 and a1
        let compactor = ThresholdCompactor::new(0.75, 2, ToolCompaction::Redact).unwrap();

        let mut conversation = vec![
            // Turn 1 - old, should be compacted
            make_tool_result_message("tool_1", "old result", TurnId::new_user_turn(1)),
            make_assistant_message(TurnId::new_assistant_turn(1)),
            // Turn 2 - recent, should be kept
            make_tool_result_message("tool_2", "new result", TurnId::new_user_turn(2)),
            make_assistant_message(TurnId::new_assistant_turn(2)),
        ];

        let summaries = std::collections::HashMap::new();
        let result = compactor.compact(&mut conversation, &summaries);

        assert_eq!(result.tool_results_redacted, 1);
        assert_eq!(result.turns_compacted, 2); // u1 and a1 compacted

        // Check that old tool result was redacted
        if let ContentBlock::ToolResult(tr) = &conversation[0].content()[0] {
            assert!(tr.content.contains("redacted"));
        } else {
            panic!("Expected ToolResult");
        }

        // Check that new tool result was kept
        if let ContentBlock::ToolResult(tr) = &conversation[2].content()[0] {
            assert_eq!(tr.content, "new result");
        } else {
            panic!("Expected ToolResult");
        }
    }

    #[test]
    fn test_compact_summarizes_with_summary() {
        // keep_recent_turns=2 keeps u2 and a2, compacts u1 and a1
        let compactor = ThresholdCompactor::new(0.75, 2, ToolCompaction::Summarize).unwrap();

        let mut conversation = vec![
            make_tool_result_message("tool_1", "very long result", TurnId::new_user_turn(1)),
            make_assistant_message(TurnId::new_assistant_turn(1)),
            make_user_message(TurnId::new_user_turn(2)),
            make_assistant_message(TurnId::new_assistant_turn(2)),
        ];

        let mut summaries = std::collections::HashMap::new();
        summaries.insert("tool_1".to_string(), "[summary]".to_string());

        let result = compactor.compact(&mut conversation, &summaries);

        assert_eq!(result.tool_results_summarized, 1);

        // Check that tool result was summarized
        if let ContentBlock::ToolResult(tr) = &conversation[0].content()[0] {
            assert_eq!(tr.content, "[summary]");
        } else {
            panic!("Expected ToolResult");
        }
    }

    // ============================================================================
    // LLMCompactorConfig Tests
    // ============================================================================

    #[test]
    fn test_llm_compactor_config_creation() {
        let config = LLMCompactorConfig::new(0.75, 5);
        assert_eq!(config.threshold, 0.75);
        assert_eq!(config.keep_recent_turns, 5);
        assert!(config.summary_system_prompt.is_none());
        assert!(config.max_summary_tokens.is_none());
        assert!(config.summary_timeout.is_none());
    }

    #[test]
    fn test_llm_compactor_config_validation() {
        // Valid config
        let config = LLMCompactorConfig::new(0.75, 5);
        assert!(config.validate().is_ok());

        // Invalid threshold (too low)
        let config = LLMCompactorConfig::new(0.0, 5);
        assert!(config.validate().is_err());

        // Invalid threshold (too high)
        let config = LLMCompactorConfig::new(1.0, 5);
        assert!(config.validate().is_err());

        // Edge case: threshold just above 0
        let config = LLMCompactorConfig::new(0.01, 5);
        assert!(config.validate().is_ok());

        // Edge case: threshold just below 1
        let config = LLMCompactorConfig::new(0.99, 5);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_llm_compactor_config_defaults() {
        let config = LLMCompactorConfig::default();
        assert_eq!(config.threshold, 0.75);
        assert_eq!(config.keep_recent_turns, 5);

        // Check that getter methods return defaults
        assert_eq!(config.system_prompt(), DEFAULT_SUMMARY_SYSTEM_PROMPT);
        assert_eq!(config.max_tokens(), DEFAULT_MAX_SUMMARY_TOKENS);
        assert_eq!(config.timeout(), DEFAULT_SUMMARY_TIMEOUT);
    }

    #[test]
    fn test_llm_compactor_config_custom_values() {
        let config = LLMCompactorConfig {
            threshold: 0.8,
            keep_recent_turns: 3,
            summary_system_prompt: Some("Custom prompt".to_string()),
            max_summary_tokens: Some(4096),
            summary_timeout: Some(Duration::from_secs(120)),
        };

        assert_eq!(config.system_prompt(), "Custom prompt");
        assert_eq!(config.max_tokens(), 4096);
        assert_eq!(config.timeout(), Duration::from_secs(120));
    }

    #[test]
    fn test_truncate_content() {
        // Short content - no truncation
        assert_eq!(truncate_content("hello", 10), "hello");

        // Exact length - no truncation
        assert_eq!(truncate_content("hello", 5), "hello");

        // Long content - truncated with ellipsis
        assert_eq!(truncate_content("hello world", 8), "hello...");

        // Very short max - edge case
        assert_eq!(truncate_content("hello", 3), "...");
    }
}
