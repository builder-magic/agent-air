//! User interaction registry for managing pending user questions.
//!
//! This module provides a registry for tools that need to wait for user input,
//! such as the AskUserQuestions tool.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::{oneshot, Mutex, mpsc};

use super::ask_user_questions::{AskUserQuestionsRequest, AskUserQuestionsResponse};
use crate::controller::types::{ControllerEvent, TurnId};

/// Maximum number of pending interactions before triggering cleanup.
const PENDING_CLEANUP_THRESHOLD: usize = 50;

/// Maximum age for pending interactions before they're considered stale (5 minutes).
const PENDING_MAX_AGE: Duration = Duration::from_secs(300);

/// Error types for user interaction operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserInteractionError {
    /// No pending interaction found for the given tool_use_id.
    NotFound,
    /// The interaction was already responded to.
    AlreadyResponded,
    /// Failed to send response (channel closed).
    SendFailed,
    /// Failed to send event notification.
    EventSendFailed,
}

impl std::fmt::Display for UserInteractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserInteractionError::NotFound => write!(f, "No pending interaction found"),
            UserInteractionError::AlreadyResponded => write!(f, "Interaction already responded to"),
            UserInteractionError::SendFailed => write!(f, "Failed to send response"),
            UserInteractionError::EventSendFailed => write!(f, "Failed to send event notification"),
        }
    }
}

impl std::error::Error for UserInteractionError {}

/// Information about a pending interaction for UI display.
#[derive(Debug, Clone)]
pub struct PendingQuestionInfo {
    /// Tool use ID for this interaction.
    pub tool_use_id: String,
    /// Session ID this interaction belongs to.
    pub session_id: i64,
    /// The questions being asked.
    pub request: AskUserQuestionsRequest,
    /// Turn ID for this interaction.
    pub turn_id: Option<TurnId>,
}

/// Internal state for a pending interaction.
struct PendingInteraction {
    session_id: i64,
    request: AskUserQuestionsRequest,
    turn_id: Option<TurnId>,
    responder: oneshot::Sender<AskUserQuestionsResponse>,
    created_at: Instant,
}

/// Registry for managing pending user interactions.
///
/// This registry tracks tools that are blocked waiting for user input
/// and provides methods for the UI to query and respond to these interactions.
pub struct UserInteractionRegistry {
    /// Pending interactions keyed by tool_use_id.
    pending: Mutex<HashMap<String, PendingInteraction>>,
    /// Channel to send events to the controller.
    event_tx: mpsc::Sender<ControllerEvent>,
}

impl UserInteractionRegistry {
    /// Create a new UserInteractionRegistry.
    ///
    /// # Arguments
    /// * `event_tx` - Channel to send events when interactions are registered.
    pub fn new(event_tx: mpsc::Sender<ControllerEvent>) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            event_tx,
        }
    }

    /// Register a pending interaction and get a receiver to await on.
    ///
    /// This is called by the AskUserQuestionsTool when it starts executing.
    /// The tool will await on the returned receiver until the UI responds.
    ///
    /// # Arguments
    /// * `tool_use_id` - Unique ID for this tool use request.
    /// * `session_id` - Session that requested the interaction.
    /// * `request` - The questions to ask the user.
    /// * `turn_id` - Optional turn ID for this interaction.
    ///
    /// # Returns
    /// A oneshot receiver that will receive the user's response.
    pub async fn register(
        &self,
        tool_use_id: String,
        session_id: i64,
        request: AskUserQuestionsRequest,
        turn_id: Option<TurnId>,
    ) -> Result<oneshot::Receiver<AskUserQuestionsResponse>, UserInteractionError> {
        let (tx, rx) = oneshot::channel();

        // Store the pending interaction
        {
            let mut pending = self.pending.lock().await;

            // Cleanup stale entries if map is getting large
            if pending.len() >= PENDING_CLEANUP_THRESHOLD {
                let now = Instant::now();
                pending.retain(|id, interaction| {
                    let keep = now.duration_since(interaction.created_at) < PENDING_MAX_AGE;
                    if !keep {
                        tracing::warn!(
                            tool_use_id = %id,
                            age_secs = now.duration_since(interaction.created_at).as_secs(),
                            "Cleaning up stale pending user interaction"
                        );
                    }
                    keep
                });
            }

            pending.insert(
                tool_use_id.clone(),
                PendingInteraction {
                    session_id,
                    request: request.clone(),
                    turn_id: turn_id.clone(),
                    responder: tx,
                    created_at: Instant::now(),
                },
            );
        }

        // Emit event to notify UI
        self.event_tx
            .send(ControllerEvent::UserInteractionRequired {
                session_id,
                tool_use_id,
                request,
                turn_id,
            })
            .await
            .map_err(|_| UserInteractionError::EventSendFailed)?;

        Ok(rx)
    }

    /// Respond to a pending interaction.
    ///
    /// This is called by the UI when the user has answered the questions.
    ///
    /// # Arguments
    /// * `tool_use_id` - ID of the tool use to respond to.
    /// * `response` - The user's answers.
    ///
    /// # Returns
    /// Ok(()) if the response was sent successfully, or an error.
    pub async fn respond(
        &self,
        tool_use_id: &str,
        response: AskUserQuestionsResponse,
    ) -> Result<(), UserInteractionError> {
        let interaction = {
            let mut pending = self.pending.lock().await;
            pending
                .remove(tool_use_id)
                .ok_or(UserInteractionError::NotFound)?
        };

        interaction
            .responder
            .send(response)
            .map_err(|_| UserInteractionError::SendFailed)
    }

    /// Cancel a pending interaction (user declined to answer).
    ///
    /// This is called by the UI when the user presses Escape to close
    /// the question panel without answering.
    ///
    /// # Arguments
    /// * `tool_use_id` - ID of the tool use to cancel.
    ///
    /// # Returns
    /// Ok(()) if the interaction was found and cancelled, or NotFound error.
    pub async fn cancel(&self, tool_use_id: &str) -> Result<(), UserInteractionError> {
        let mut pending = self.pending.lock().await;
        if pending.remove(tool_use_id).is_some() {
            // Dropping the sender will cause the tool to receive a RecvError
            // which will be converted to "User declined to answer"
            Ok(())
        } else {
            Err(UserInteractionError::NotFound)
        }
    }

    /// Get all pending interactions for a session.
    ///
    /// This is called by the UI when switching sessions to display
    /// any pending questions for that session.
    ///
    /// # Arguments
    /// * `session_id` - Session ID to query.
    ///
    /// # Returns
    /// List of pending question information for the session.
    pub async fn pending_for_session(&self, session_id: i64) -> Vec<PendingQuestionInfo> {
        let pending = self.pending.lock().await;
        pending
            .iter()
            .filter(|(_, interaction)| interaction.session_id == session_id)
            .map(|(tool_use_id, interaction)| PendingQuestionInfo {
                tool_use_id: tool_use_id.clone(),
                session_id: interaction.session_id,
                request: interaction.request.clone(),
                turn_id: interaction.turn_id.clone(),
            })
            .collect()
    }

    /// Cancel all pending interactions for a session.
    ///
    /// This is called when a session is destroyed. It drops the senders,
    /// which will cause the awaiting tools to receive a RecvError.
    ///
    /// # Arguments
    /// * `session_id` - Session ID to cancel.
    pub async fn cancel_session(&self, session_id: i64) {
        let mut pending = self.pending.lock().await;
        pending.retain(|_, interaction| interaction.session_id != session_id);
        // Dropped senders will cause RecvError on the tool side
    }

    /// Check if there are any pending interactions for a session.
    ///
    /// # Arguments
    /// * `session_id` - Session ID to check.
    ///
    /// # Returns
    /// True if there are pending interactions.
    pub async fn has_pending(&self, session_id: i64) -> bool {
        let pending = self.pending.lock().await;
        pending
            .values()
            .any(|interaction| interaction.session_id == session_id)
    }

    /// Get the count of pending interactions.
    pub async fn pending_count(&self) -> usize {
        let pending = self.pending.lock().await;
        pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::tools::ask_user_questions::{Answer, Question};

    fn create_test_request() -> AskUserQuestionsRequest {
        AskUserQuestionsRequest {
            questions: vec![Question::SingleChoice {
                text: "Which option?".to_string(),
                choices: vec!["Option A".to_string(), "Option B".to_string()],
                required: true,
            }],
        }
    }

    fn create_test_response() -> AskUserQuestionsResponse {
        AskUserQuestionsResponse {
            answers: vec![Answer {
                question: "Which option?".to_string(),
                answer: vec!["Option A".to_string()],
            }],
        }
    }

    #[tokio::test]
    async fn test_register_and_respond() {
        let (event_tx, mut event_rx) = mpsc::channel(10);
        let registry = UserInteractionRegistry::new(event_tx);

        let request = create_test_request();
        let response = create_test_response();

        // Register interaction
        let rx = registry
            .register("tool_123".to_string(), 1, request.clone(), None)
            .await
            .unwrap();

        // Verify event was emitted
        let event = event_rx.recv().await.unwrap();
        if let ControllerEvent::UserInteractionRequired {
            session_id,
            tool_use_id,
            ..
        } = event
        {
            assert_eq!(session_id, 1);
            assert_eq!(tool_use_id, "tool_123");
        } else {
            panic!("Expected UserInteractionRequired event");
        }

        // Respond to interaction
        registry
            .respond("tool_123", response.clone())
            .await
            .unwrap();

        // Verify response was received
        let received = rx.await.unwrap();
        assert_eq!(received.answers.len(), 1);
    }

    #[tokio::test]
    async fn test_respond_not_found() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = UserInteractionRegistry::new(event_tx);

        let response = create_test_response();
        let result = registry.respond("nonexistent", response).await;

        assert_eq!(result, Err(UserInteractionError::NotFound));
    }

    #[tokio::test]
    async fn test_pending_for_session() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = UserInteractionRegistry::new(event_tx);

        let request = create_test_request();

        // Register interactions for different sessions
        let _ = registry
            .register("tool_1".to_string(), 1, request.clone(), None)
            .await;
        let _ = registry
            .register("tool_2".to_string(), 1, request.clone(), None)
            .await;
        let _ = registry
            .register("tool_3".to_string(), 2, request.clone(), None)
            .await;

        // Query session 1
        let pending = registry.pending_for_session(1).await;
        assert_eq!(pending.len(), 2);

        // Query session 2
        let pending = registry.pending_for_session(2).await;
        assert_eq!(pending.len(), 1);

        // Query nonexistent session
        let pending = registry.pending_for_session(999).await;
        assert_eq!(pending.len(), 0);
    }

    #[tokio::test]
    async fn test_cancel_session() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = UserInteractionRegistry::new(event_tx);

        let request = create_test_request();

        // Register interactions
        let rx1 = registry
            .register("tool_1".to_string(), 1, request.clone(), None)
            .await
            .unwrap();
        let _ = registry
            .register("tool_2".to_string(), 2, request.clone(), None)
            .await;

        // Cancel session 1
        registry.cancel_session(1).await;

        // Session 1 interaction should be gone
        assert!(!registry.has_pending(1).await);
        assert!(registry.has_pending(2).await);

        // Receiver should get error
        assert!(rx1.await.is_err());
    }

    #[tokio::test]
    async fn test_has_pending() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = UserInteractionRegistry::new(event_tx);

        assert!(!registry.has_pending(1).await);

        let request = create_test_request();
        let _ = registry
            .register("tool_1".to_string(), 1, request, None)
            .await;

        assert!(registry.has_pending(1).await);
        assert!(!registry.has_pending(2).await);
    }

    #[tokio::test]
    async fn test_pending_count() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = UserInteractionRegistry::new(event_tx);

        assert_eq!(registry.pending_count().await, 0);

        let request = create_test_request();
        let _ = registry
            .register("tool_1".to_string(), 1, request.clone(), None)
            .await;
        assert_eq!(registry.pending_count().await, 1);

        let _ = registry
            .register("tool_2".to_string(), 1, request, None)
            .await;
        assert_eq!(registry.pending_count().await, 2);
    }
}
