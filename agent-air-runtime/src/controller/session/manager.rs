// This implements a session manager that handles multiple sessions

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use super::LLMSession;
use super::config::LLMSessionConfig;
use crate::client::error::LlmError;
use crate::controller::types::FromLLMPayload;

/// Manages multiple LLM sessions
pub struct LLMSessionManager {
    /// Map of session ID to session instance
    sessions: RwLock<HashMap<i64, Arc<LLMSession>>>,
}

impl LLMSessionManager {
    /// Creates a new session manager
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Creates a new LLM session and starts it.
    ///
    /// # Arguments
    /// * `config` - Session configuration (includes model, API key, etc.)
    /// * `from_llm` - Channel sender for responses from the LLM
    /// * `channel_size` - Buffer size for the session's input channel
    ///
    /// # Returns
    /// The session ID of the newly created session
    ///
    /// # Errors
    /// Returns an error if the session fails to initialize (e.g., TLS setup failure)
    pub async fn create_session(
        &self,
        config: LLMSessionConfig,
        from_llm: mpsc::Sender<FromLLMPayload>,
        channel_size: usize,
    ) -> Result<i64, LlmError> {
        let cancel_token = CancellationToken::new();
        let session = Arc::new(LLMSession::new(
            config,
            from_llm,
            cancel_token,
            channel_size,
        )?);
        let session_id = session.id();

        // Store the session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, Arc::clone(&session));
        }

        // Spawn the session's processing loop
        let session_clone = Arc::clone(&session);
        tokio::spawn(async move {
            session_clone.start().await;
        });

        tracing::info!(session_id, "Session created and started");
        Ok(session_id)
    }

    /// Retrieves a session by its ID.
    ///
    /// # Returns
    /// The session if found, None otherwise
    pub async fn get_session_by_id(&self, session_id: i64) -> Option<Arc<LLMSession>> {
        let sessions = self.sessions.read().await;
        sessions.get(&session_id).cloned()
    }

    /// Removes and shuts down a specific session.
    ///
    /// # Arguments
    /// * `session_id` - The ID of the session to remove
    ///
    /// # Returns
    /// true if the session was found and removed, false otherwise
    pub async fn remove_session(&self, session_id: i64) -> bool {
        let session = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&session_id)
        };

        if let Some(session) = session {
            session.shutdown();
            tracing::info!(session_id, "Session removed");
            true
        } else {
            false
        }
    }

    /// Shuts down all sessions managed by this manager.
    /// This is idempotent and safe to call multiple times.
    pub async fn shutdown(&self) {
        let sessions: Vec<Arc<LLMSession>> = {
            let mut sessions = self.sessions.write().await;
            sessions.drain().map(|(_, s)| s).collect()
        };

        for session in sessions {
            session.shutdown();
        }

        tracing::info!("Session manager shutdown complete");
    }

    /// Returns the number of active sessions
    pub async fn session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }
}

impl Default for LLMSessionManager {
    fn default() -> Self {
        Self::new()
    }
}
