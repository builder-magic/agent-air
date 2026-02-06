use std::collections::HashMap;

use tokio::sync::RwLock;

/// Token meter for tracking input and output tokens.
#[derive(Debug, Clone, Default)]
pub struct TokenMeter {
    /// Number of input tokens.
    pub input_tokens: i64,
    /// Number of output tokens.
    pub output_tokens: i64,
}

impl TokenMeter {
    /// Create a new empty token meter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a token meter with initial values.
    pub fn with_values(input_tokens: i64, output_tokens: i64) -> Self {
        Self {
            input_tokens,
            output_tokens,
        }
    }

    /// Returns the total tokens (input + output).
    pub fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens
    }

    /// Add tokens from another meter.
    pub fn add(&mut self, other: &TokenMeter) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
    }
}

/// Thread-safe token usage tracker.
/// Tracks usage by session, model, and total.
pub struct TokenUsageTracker {
    tokens_per_session: RwLock<HashMap<i64, TokenMeter>>,
    tokens_per_model: RwLock<HashMap<String, TokenMeter>>,
    total_usage: RwLock<TokenMeter>,
}

impl TokenUsageTracker {
    /// Create a new token usage tracker.
    pub fn new() -> Self {
        Self {
            tokens_per_session: RwLock::new(HashMap::new()),
            tokens_per_model: RwLock::new(HashMap::new()),
            total_usage: RwLock::new(TokenMeter::new()),
        }
    }

    /// Increment token usage for a session and model.
    pub async fn increment(
        &self,
        session_id: i64,
        model: &str,
        input_tokens: i64,
        output_tokens: i64,
    ) {
        // Update session usage
        {
            let mut sessions = self.tokens_per_session.write().await;
            let meter = sessions.entry(session_id).or_insert_with(TokenMeter::new);
            meter.input_tokens += input_tokens;
            meter.output_tokens += output_tokens;
        }

        // Update model usage
        {
            let mut models = self.tokens_per_model.write().await;
            let meter = models
                .entry(model.to_string())
                .or_insert_with(TokenMeter::new);
            meter.input_tokens += input_tokens;
            meter.output_tokens += output_tokens;
        }

        // Update total usage
        {
            let mut total = self.total_usage.write().await;
            total.input_tokens += input_tokens;
            total.output_tokens += output_tokens;
        }
    }

    /// Get token usage for a specific session.
    /// Returns None if the session has no recorded usage.
    pub async fn get_session_usage(&self, session_id: i64) -> Option<TokenMeter> {
        let sessions = self.tokens_per_session.read().await;
        sessions.get(&session_id).cloned()
    }

    /// Get token usage for a specific model.
    /// Returns None if the model has no recorded usage.
    pub async fn get_model_usage(&self, model: &str) -> Option<TokenMeter> {
        let models = self.tokens_per_model.read().await;
        models.get(model).cloned()
    }

    /// Get total token usage across all sessions and models.
    pub async fn get_total_usage(&self) -> TokenMeter {
        let total = self.total_usage.read().await;
        total.clone()
    }

    /// Get all session usage as a map.
    pub async fn get_all_session_usage(&self) -> HashMap<i64, TokenMeter> {
        let sessions = self.tokens_per_session.read().await;
        sessions.clone()
    }

    /// Get all model usage as a map.
    pub async fn get_all_model_usage(&self) -> HashMap<String, TokenMeter> {
        let models = self.tokens_per_model.read().await;
        models.clone()
    }

    /// Remove usage tracking for a session (e.g., when session is deleted).
    pub async fn remove_session(&self, session_id: i64) {
        let mut sessions = self.tokens_per_session.write().await;
        sessions.remove(&session_id);
    }

    /// Get the number of tracked sessions.
    pub async fn session_count(&self) -> usize {
        let sessions = self.tokens_per_session.read().await;
        sessions.len()
    }

    /// Get the number of tracked models.
    pub async fn model_count(&self) -> usize {
        let models = self.tokens_per_model.read().await;
        models.len()
    }
}

impl Default for TokenUsageTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_increment_usage() {
        let tracker = TokenUsageTracker::new();

        tracker.increment(1, "claude-3-sonnet", 100, 50).await;
        tracker.increment(1, "claude-3-sonnet", 200, 100).await;
        tracker.increment(2, "gpt-4", 150, 75).await;

        // Check session usage
        let session1 = tracker.get_session_usage(1).await.unwrap();
        assert_eq!(session1.input_tokens, 300);
        assert_eq!(session1.output_tokens, 150);

        let session2 = tracker.get_session_usage(2).await.unwrap();
        assert_eq!(session2.input_tokens, 150);
        assert_eq!(session2.output_tokens, 75);

        // Check model usage
        let claude = tracker.get_model_usage("claude-3-sonnet").await.unwrap();
        assert_eq!(claude.input_tokens, 300);
        assert_eq!(claude.output_tokens, 150);

        let gpt4 = tracker.get_model_usage("gpt-4").await.unwrap();
        assert_eq!(gpt4.input_tokens, 150);
        assert_eq!(gpt4.output_tokens, 75);

        // Check total usage
        let total = tracker.get_total_usage().await;
        assert_eq!(total.input_tokens, 450);
        assert_eq!(total.output_tokens, 225);
    }

    #[tokio::test]
    async fn test_nonexistent_session() {
        let tracker = TokenUsageTracker::new();

        let usage = tracker.get_session_usage(999).await;
        assert!(usage.is_none());
    }

    #[tokio::test]
    async fn test_remove_session() {
        let tracker = TokenUsageTracker::new();

        tracker.increment(1, "model", 100, 50).await;
        assert!(tracker.get_session_usage(1).await.is_some());

        tracker.remove_session(1).await;
        assert!(tracker.get_session_usage(1).await.is_none());

        // Total should still reflect the removed session's usage
        let total = tracker.get_total_usage().await;
        assert_eq!(total.input_tokens, 100);
    }

    #[tokio::test]
    async fn test_token_meter() {
        let meter = TokenMeter::with_values(100, 50);
        assert_eq!(meter.total_tokens(), 150);

        let mut meter2 = TokenMeter::new();
        meter2.add(&meter);
        assert_eq!(meter2.input_tokens, 100);
        assert_eq!(meter2.output_tokens, 50);
    }
}
