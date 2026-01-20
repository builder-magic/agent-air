//! Permission registry for managing permission requests and session-level grants.
//!
//! This module provides a registry for tools that need to request user permission,
//! such as the AskForPermissions tool.

use std::collections::{HashMap, HashSet};

use tokio::sync::{mpsc, oneshot, Mutex};

use super::ask_for_permissions::{PermissionCategory, PermissionRequest, PermissionResponse};
use crate::controller::types::{ControllerEvent, TurnId};

/// Error types for permission operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionError {
    /// No pending permission request found for the given tool_use_id.
    NotFound,
    /// The permission request was already responded to.
    AlreadyResponded,
    /// Failed to send response (channel closed).
    SendFailed,
    /// Failed to send event notification.
    EventSendFailed,
}

impl std::fmt::Display for PermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionError::NotFound => write!(f, "No pending permission request found"),
            PermissionError::AlreadyResponded => write!(f, "Permission already responded to"),
            PermissionError::SendFailed => write!(f, "Failed to send response"),
            PermissionError::EventSendFailed => write!(f, "Failed to send event notification"),
        }
    }
}

impl std::error::Error for PermissionError {}

/// Information about a pending permission request for UI display.
#[derive(Debug, Clone)]
pub struct PendingPermissionInfo {
    /// Tool use ID for this permission request.
    pub tool_use_id: String,
    /// Session ID this permission belongs to.
    pub session_id: i64,
    /// The permission request details.
    pub request: PermissionRequest,
    /// Turn ID for this permission request.
    pub turn_id: Option<TurnId>,
}

/// A grant that was approved for the session.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct PermissionGrant {
    /// Category of the permission.
    pub category: PermissionCategory,
    /// The action pattern that was granted (for matching future requests).
    pub action_pattern: String,
}

/// Internal state for a pending permission request.
struct PendingPermission {
    session_id: i64,
    request: PermissionRequest,
    turn_id: Option<TurnId>,
    responder: oneshot::Sender<PermissionResponse>,
}

/// Registry for managing permission requests and session-level grants.
///
/// This registry tracks tools that are blocked waiting for permission
/// and provides methods for the UI to query and respond to these requests.
/// It also caches session-level grants to avoid re-asking.
pub struct PermissionRegistry {
    /// Pending permission requests keyed by tool_use_id.
    pending: Mutex<HashMap<String, PendingPermission>>,
    /// Session-level grants (session_id -> set of grants).
    session_grants: Mutex<HashMap<i64, HashSet<PermissionGrant>>>,
    /// Channel to send events to the controller.
    event_tx: mpsc::Sender<ControllerEvent>,
}

impl PermissionRegistry {
    /// Create a new PermissionRegistry.
    ///
    /// # Arguments
    /// * `event_tx` - Channel to send events when permissions are requested.
    pub fn new(event_tx: mpsc::Sender<ControllerEvent>) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            session_grants: Mutex::new(HashMap::new()),
            event_tx,
        }
    }

    /// Check if permission is already granted for the session.
    ///
    /// This checks if a previous session-level grant covers this request.
    ///
    /// # Arguments
    /// * `session_id` - Session to check.
    /// * `request` - The permission request to check.
    ///
    /// # Returns
    /// True if permission was previously granted for the session.
    pub async fn is_granted(&self, session_id: i64, request: &PermissionRequest) -> bool {
        let grants = self.session_grants.lock().await;
        if let Some(session_grants) = grants.get(&session_id) {
            // Check if any grant matches the request
            session_grants.iter().any(|grant| {
                grant.category == request.category && grant.action_pattern == request.action
            })
        } else {
            false
        }
    }

    /// Register a permission request and get a receiver to await on.
    ///
    /// This is called by the AskForPermissionsTool when it starts executing.
    /// The tool will await on the returned receiver until the UI responds.
    ///
    /// # Arguments
    /// * `tool_use_id` - Unique ID for this tool use request.
    /// * `session_id` - Session that requested the permission.
    /// * `request` - The permission request details.
    /// * `turn_id` - Optional turn ID for this request.
    ///
    /// # Returns
    /// A oneshot receiver that will receive the user's response.
    pub async fn register(
        &self,
        tool_use_id: String,
        session_id: i64,
        request: PermissionRequest,
        turn_id: Option<TurnId>,
    ) -> Result<oneshot::Receiver<PermissionResponse>, PermissionError> {
        let (tx, rx) = oneshot::channel();

        // Store the pending request
        {
            let mut pending = self.pending.lock().await;
            pending.insert(
                tool_use_id.clone(),
                PendingPermission {
                    session_id,
                    request: request.clone(),
                    turn_id: turn_id.clone(),
                    responder: tx,
                },
            );
        }

        // Emit event to notify UI
        self.event_tx
            .send(ControllerEvent::PermissionRequired {
                session_id,
                tool_use_id,
                request,
                turn_id,
            })
            .await
            .map_err(|_| PermissionError::EventSendFailed)?;

        Ok(rx)
    }

    /// Respond to a pending permission request.
    ///
    /// This is called by the UI when the user has granted or denied permission.
    ///
    /// # Arguments
    /// * `tool_use_id` - ID of the tool use to respond to.
    /// * `response` - The user's response (grant/deny).
    ///
    /// # Returns
    /// Ok(()) if the response was sent successfully, or an error.
    pub async fn respond(
        &self,
        tool_use_id: &str,
        response: PermissionResponse,
    ) -> Result<(), PermissionError> {
        let pending_permission = {
            let mut pending = self.pending.lock().await;
            pending
                .remove(tool_use_id)
                .ok_or(PermissionError::NotFound)?
        };

        // If granted with session scope, cache the grant
        if response.granted {
            if let Some(ref scope) = response.scope {
                if *scope == super::ask_for_permissions::PermissionScope::Session {
                    let mut grants = self.session_grants.lock().await;
                    let session_grants = grants
                        .entry(pending_permission.session_id)
                        .or_insert_with(HashSet::new);
                    session_grants.insert(PermissionGrant {
                        category: pending_permission.request.category.clone(),
                        action_pattern: pending_permission.request.action.clone(),
                    });
                }
            }
        }

        pending_permission
            .responder
            .send(response)
            .map_err(|_| PermissionError::SendFailed)
    }

    /// Cancel a pending permission request (user declined).
    ///
    /// This is called by the UI when the user closes the permission dialog
    /// without responding.
    ///
    /// # Arguments
    /// * `tool_use_id` - ID of the tool use to cancel.
    ///
    /// # Returns
    /// Ok(()) if the request was found and cancelled, or NotFound error.
    pub async fn cancel(&self, tool_use_id: &str) -> Result<(), PermissionError> {
        let mut pending = self.pending.lock().await;
        if pending.remove(tool_use_id).is_some() {
            // Dropping the sender will cause the tool to receive a RecvError
            // which will be converted to "User denied permission"
            Ok(())
        } else {
            Err(PermissionError::NotFound)
        }
    }

    /// Get all pending permission requests for a session.
    ///
    /// This is called by the UI when switching sessions to display
    /// any pending permission requests for that session.
    ///
    /// # Arguments
    /// * `session_id` - Session ID to query.
    ///
    /// # Returns
    /// List of pending permission info for the session.
    pub async fn pending_for_session(&self, session_id: i64) -> Vec<PendingPermissionInfo> {
        let pending = self.pending.lock().await;
        pending
            .iter()
            .filter(|(_, perm)| perm.session_id == session_id)
            .map(|(tool_use_id, perm)| PendingPermissionInfo {
                tool_use_id: tool_use_id.clone(),
                session_id: perm.session_id,
                request: perm.request.clone(),
                turn_id: perm.turn_id.clone(),
            })
            .collect()
    }

    /// Cancel all pending permission requests for a session.
    ///
    /// This is called when a session is destroyed. It drops the senders,
    /// which will cause the awaiting tools to receive a RecvError.
    ///
    /// # Arguments
    /// * `session_id` - Session ID to cancel.
    pub async fn cancel_session(&self, session_id: i64) {
        let mut pending = self.pending.lock().await;
        pending.retain(|_, perm| perm.session_id != session_id);
        // Dropped senders will cause RecvError on the tool side
    }

    /// Clear all grants for a session.
    ///
    /// This is called when a session ends or is reset.
    ///
    /// # Arguments
    /// * `session_id` - Session ID to clear grants for.
    pub async fn clear_session(&self, session_id: i64) {
        // Cancel pending requests
        self.cancel_session(session_id).await;

        // Clear session grants
        let mut grants = self.session_grants.lock().await;
        grants.remove(&session_id);
    }

    /// Check if there are any pending permission requests for a session.
    ///
    /// # Arguments
    /// * `session_id` - Session ID to check.
    ///
    /// # Returns
    /// True if there are pending permission requests.
    pub async fn has_pending(&self, session_id: i64) -> bool {
        let pending = self.pending.lock().await;
        pending.values().any(|perm| perm.session_id == session_id)
    }

    /// Get the count of pending permission requests.
    pub async fn pending_count(&self) -> usize {
        let pending = self.pending.lock().await;
        pending.len()
    }

    /// Get all session grants for a session.
    ///
    /// # Arguments
    /// * `session_id` - Session ID to query.
    ///
    /// # Returns
    /// Set of grants for the session (empty if none).
    pub async fn session_grants(&self, session_id: i64) -> HashSet<PermissionGrant> {
        let grants = self.session_grants.lock().await;
        grants.get(&session_id).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::tools::ask_for_permissions::PermissionScope;

    fn create_test_request() -> PermissionRequest {
        PermissionRequest {
            action: "Delete file /tmp/foo.txt".to_string(),
            reason: Some("User requested cleanup".to_string()),
            resources: vec!["/tmp/foo.txt".to_string()],
            category: PermissionCategory::FileDelete,
        }
    }

    fn create_grant_response() -> PermissionResponse {
        PermissionResponse {
            granted: true,
            scope: Some(PermissionScope::Session),
            message: None,
        }
    }

    fn create_deny_response() -> PermissionResponse {
        PermissionResponse {
            granted: false,
            scope: None,
            message: Some("Not allowed".to_string()),
        }
    }

    #[tokio::test]
    async fn test_register_and_respond() {
        let (event_tx, mut event_rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(event_tx);

        let request = create_test_request();
        let response = create_grant_response();

        // Register permission request
        let rx = registry
            .register("tool_123".to_string(), 1, request.clone(), None)
            .await
            .unwrap();

        // Verify event was emitted
        let event = event_rx.recv().await.unwrap();
        if let ControllerEvent::PermissionRequired {
            session_id,
            tool_use_id,
            ..
        } = event
        {
            assert_eq!(session_id, 1);
            assert_eq!(tool_use_id, "tool_123");
        } else {
            panic!("Expected PermissionRequired event");
        }

        // Respond to request
        registry
            .respond("tool_123", response.clone())
            .await
            .unwrap();

        // Verify response was received
        let received = rx.await.unwrap();
        assert!(received.granted);
        assert_eq!(received.scope, Some(PermissionScope::Session));
    }

    #[tokio::test]
    async fn test_respond_not_found() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(event_tx);

        let response = create_grant_response();
        let result = registry.respond("nonexistent", response).await;

        assert_eq!(result, Err(PermissionError::NotFound));
    }

    #[tokio::test]
    async fn test_session_grant_caching() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(event_tx);

        let request = create_test_request();

        // Not granted initially
        assert!(!registry.is_granted(1, &request).await);

        // Register and grant with session scope
        let rx = registry
            .register("tool_1".to_string(), 1, request.clone(), None)
            .await
            .unwrap();
        registry.respond("tool_1", create_grant_response()).await.unwrap();

        // Consume the response to complete the flow
        let _ = rx.await;

        // Now should be granted
        assert!(registry.is_granted(1, &request).await);

        // Different session should not be granted
        assert!(!registry.is_granted(2, &request).await);
    }

    #[tokio::test]
    async fn test_once_scope_not_cached() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(event_tx);

        let request = create_test_request();

        // Register and grant with Once scope
        let rx = registry
            .register("tool_1".to_string(), 1, request.clone(), None)
            .await
            .unwrap();
        registry
            .respond(
                "tool_1",
                PermissionResponse {
                    granted: true,
                    scope: Some(PermissionScope::Once),
                    message: None,
                },
            )
            .await
            .unwrap();

        // Consume the response to complete the flow
        let _ = rx.await;

        // Should NOT be cached (Once scope)
        assert!(!registry.is_granted(1, &request).await);
    }

    #[tokio::test]
    async fn test_denied_not_cached() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(event_tx);

        let request = create_test_request();

        // Register and deny
        let rx = registry
            .register("tool_1".to_string(), 1, request.clone(), None)
            .await
            .unwrap();
        registry.respond("tool_1", create_deny_response()).await.unwrap();

        // Consume the response to complete the flow
        let _ = rx.await;

        // Should not be granted
        assert!(!registry.is_granted(1, &request).await);
    }

    #[tokio::test]
    async fn test_pending_for_session() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(event_tx);

        let request = create_test_request();

        // Register requests for different sessions
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
        let registry = PermissionRegistry::new(event_tx);

        let request = create_test_request();

        // Register and grant for session 1
        let rx1 = registry
            .register("tool_1".to_string(), 1, request.clone(), None)
            .await
            .unwrap();
        registry.respond("tool_1", create_grant_response()).await.unwrap();

        // Consume the response to complete the flow
        let _ = rx1.await;

        // Register request for session 2
        let rx2 = registry
            .register("tool_2".to_string(), 2, request.clone(), None)
            .await
            .unwrap();

        // Clear session 1
        registry.clear_session(1).await;

        // Session 1 grants should be gone
        assert!(!registry.is_granted(1, &request).await);

        // Session 2 should still have pending
        assert!(registry.has_pending(2).await);

        // Session 2 receiver should still work
        registry.respond("tool_2", create_grant_response()).await.unwrap();
        let received = rx2.await.unwrap();
        assert!(received.granted);
    }

    #[tokio::test]
    async fn test_has_pending() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(event_tx);

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
        let registry = PermissionRegistry::new(event_tx);

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

    #[tokio::test]
    async fn test_cancel() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(event_tx);

        let request = create_test_request();
        let rx = registry
            .register("tool_1".to_string(), 1, request, None)
            .await
            .unwrap();

        // Cancel the request
        registry.cancel("tool_1").await.unwrap();

        // Receiver should get error
        assert!(rx.await.is_err());

        // Cancel nonexistent should fail
        let result = registry.cancel("nonexistent").await;
        assert_eq!(result, Err(PermissionError::NotFound));
    }
}
