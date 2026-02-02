//! Permission registry for managing grants and permission requests.
//!
//! The registry tracks:
//! - Active grants per session
//! - Pending permission requests (both individual and batched)
//! - Provides methods for checking, granting, and revoking permissions

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{mpsc, oneshot, Mutex};

use super::{
    BatchPermissionRequest, BatchPermissionResponse, Grant, GrantTarget, PermissionLevel,
    PermissionRequest,
};

/// Counter for generating unique batch IDs.
static BATCH_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generates a unique batch ID.
pub fn generate_batch_id() -> String {
    let id = BATCH_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("batch-{}", id)
}

/// Error types for permission operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionError {
    /// No pending permission request found.
    NotFound,
    /// The permission request was already responded to.
    AlreadyResponded,
    /// Failed to send response (channel closed).
    SendFailed,
    /// Failed to send event notification.
    EventSendFailed,
    /// The batch has already been processed.
    BatchAlreadyProcessed,
}

impl std::fmt::Display for PermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionError::NotFound => write!(f, "No pending permission request found"),
            PermissionError::AlreadyResponded => write!(f, "Permission already responded to"),
            PermissionError::SendFailed => write!(f, "Failed to send response"),
            PermissionError::EventSendFailed => write!(f, "Failed to send event notification"),
            PermissionError::BatchAlreadyProcessed => write!(f, "Batch has already been processed"),
        }
    }
}

impl std::error::Error for PermissionError {}

/// Event emitted when permission is needed.
#[derive(Debug, Clone)]
pub enum PermissionEvent {
    /// A single permission request requires user approval.
    PermissionRequired {
        session_id: i64,
        request_id: String,
        request: PermissionRequest,
    },
    /// A batch of permission requests requires user approval.
    BatchPermissionRequired {
        session_id: i64,
        batch: BatchPermissionRequest,
    },
}

/// Internal state for a pending individual permission request.
struct PendingRequest {
    session_id: i64,
    #[allow(dead_code)] // Stored for future use in UI display
    request: PermissionRequest,
    responder: oneshot::Sender<bool>,
}

/// Internal state for a pending batch permission request.
struct PendingBatch {
    session_id: i64,
    #[allow(dead_code)] // Stored for future use in UI display
    requests: Vec<PermissionRequest>,
    responder: oneshot::Sender<BatchPermissionResponse>,
}

/// Registry for managing permission grants and requests.
///
/// This registry implements the new grant-based permission system with:
/// - Hierarchical permission levels (Admin > Execute > Write > Read > None)
/// - Path-based grants with optional recursion
/// - Domain and command pattern grants
/// - Batch permission request handling
pub struct PermissionRegistry {
    /// Active grants per session (session_id -> list of grants).
    session_grants: Mutex<HashMap<i64, Vec<Grant>>>,
    /// Pending individual permission requests (request_id -> pending state).
    pending_requests: Mutex<HashMap<String, PendingRequest>>,
    /// Pending batch permission requests (batch_id -> pending state).
    pending_batches: Mutex<HashMap<String, PendingBatch>>,
    /// Channel to send permission events.
    event_tx: mpsc::Sender<PermissionEvent>,
}

impl PermissionRegistry {
    /// Creates a new PermissionRegistry.
    ///
    /// # Arguments
    /// * `event_tx` - Channel to send events when permissions are requested.
    pub fn new(event_tx: mpsc::Sender<PermissionEvent>) -> Self {
        Self {
            session_grants: Mutex::new(HashMap::new()),
            pending_requests: Mutex::new(HashMap::new()),
            pending_batches: Mutex::new(HashMap::new()),
            event_tx,
        }
    }

    // ========================================================================
    // Grant Management
    // ========================================================================

    /// Adds a grant for a session.
    ///
    /// # Arguments
    /// * `session_id` - Session to add the grant to.
    /// * `grant` - The grant to add.
    pub async fn add_grant(&self, session_id: i64, grant: Grant) {
        let mut grants = self.session_grants.lock().await;
        let session_grants = grants.entry(session_id).or_insert_with(Vec::new);
        session_grants.push(grant);
    }

    /// Adds multiple grants for a session.
    ///
    /// # Arguments
    /// * `session_id` - Session to add the grants to.
    /// * `new_grants` - The grants to add.
    pub async fn add_grants(&self, session_id: i64, new_grants: Vec<Grant>) {
        let mut grants = self.session_grants.lock().await;
        let session_grants = grants.entry(session_id).or_insert_with(Vec::new);
        session_grants.extend(new_grants);
    }

    /// Removes expired grants from a session.
    pub async fn cleanup_expired(&self, session_id: i64) {
        let mut grants = self.session_grants.lock().await;
        if let Some(session_grants) = grants.get_mut(&session_id) {
            session_grants.retain(|g| !g.is_expired());
        }
    }

    /// Revokes all grants matching a specific target for a session.
    ///
    /// # Arguments
    /// * `session_id` - Session to revoke grants from.
    /// * `target` - Target to match for revocation.
    ///
    /// # Returns
    /// Number of grants revoked.
    pub async fn revoke_grants(&self, session_id: i64, target: &GrantTarget) -> usize {
        let mut grants = self.session_grants.lock().await;
        if let Some(session_grants) = grants.get_mut(&session_id) {
            let original_len = session_grants.len();
            session_grants.retain(|g| &g.target != target);
            original_len - session_grants.len()
        } else {
            0
        }
    }

    /// Gets all grants for a session.
    ///
    /// # Arguments
    /// * `session_id` - Session to query.
    ///
    /// # Returns
    /// List of grants for the session (empty if none).
    pub async fn get_grants(&self, session_id: i64) -> Vec<Grant> {
        let grants = self.session_grants.lock().await;
        grants.get(&session_id).cloned().unwrap_or_default()
    }

    /// Clears all grants for a session.
    ///
    /// # Arguments
    /// * `session_id` - Session to clear.
    pub async fn clear_grants(&self, session_id: i64) {
        let mut grants = self.session_grants.lock().await;
        grants.remove(&session_id);
    }

    // ========================================================================
    // Permission Checking
    // ========================================================================

    /// Checks if a permission request is satisfied by existing grants.
    ///
    /// This checks if any grant in the session satisfies the request based on:
    /// 1. Target coverage (grant target covers request target)
    /// 2. Level hierarchy (grant level >= required level)
    /// 3. Grant not expired
    ///
    /// # Arguments
    /// * `session_id` - Session to check.
    /// * `request` - The permission request to check.
    ///
    /// # Returns
    /// `true` if permission is granted, `false` otherwise.
    pub async fn check(&self, session_id: i64, request: &PermissionRequest) -> bool {
        let grants = self.session_grants.lock().await;
        if let Some(session_grants) = grants.get(&session_id) {
            session_grants.iter().any(|grant| grant.satisfies(request))
        } else {
            false
        }
    }

    /// Checks multiple permission requests and returns which are already granted.
    ///
    /// # Arguments
    /// * `session_id` - Session to check.
    /// * `requests` - The permission requests to check.
    ///
    /// # Returns
    /// Set of request IDs that are already granted.
    pub async fn check_batch(
        &self,
        session_id: i64,
        requests: &[PermissionRequest],
    ) -> HashSet<String> {
        let grants = self.session_grants.lock().await;
        let session_grants = grants.get(&session_id);

        let mut granted = HashSet::new();
        for request in requests {
            if let Some(sg) = session_grants {
                if sg.iter().any(|grant| grant.satisfies(request)) {
                    granted.insert(request.id.clone());
                }
            }
        }
        granted
    }

    /// Finds which grant (if any) satisfies a request.
    ///
    /// # Arguments
    /// * `session_id` - Session to check.
    /// * `request` - The permission request to check.
    ///
    /// # Returns
    /// The grant that satisfies the request, if any.
    pub async fn find_satisfying_grant(
        &self,
        session_id: i64,
        request: &PermissionRequest,
    ) -> Option<Grant> {
        let grants = self.session_grants.lock().await;
        if let Some(session_grants) = grants.get(&session_id) {
            session_grants
                .iter()
                .find(|grant| grant.satisfies(request))
                .cloned()
        } else {
            None
        }
    }

    // ========================================================================
    // Individual Permission Requests
    // ========================================================================

    /// Registers an individual permission request.
    ///
    /// If the request is already satisfied by existing grants, returns `Ok(true)` immediately.
    /// Otherwise, registers the request, emits an event, and returns a receiver to await the response.
    ///
    /// # Arguments
    /// * `session_id` - Session requesting permission.
    /// * `request` - The permission request.
    ///
    /// # Returns
    /// A receiver that will receive `true` if granted, `false` if denied.
    pub async fn request_permission(
        &self,
        session_id: i64,
        request: PermissionRequest,
    ) -> Result<oneshot::Receiver<bool>, PermissionError> {
        // Check if already granted
        if self.check(session_id, &request).await {
            let (tx, rx) = oneshot::channel();
            let _ = tx.send(true); // Auto-approve
            return Ok(rx);
        }

        let (tx, rx) = oneshot::channel();
        let request_id = request.id.clone();

        // Store pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(
                request_id.clone(),
                PendingRequest {
                    session_id,
                    request: request.clone(),
                    responder: tx,
                },
            );
        }

        // Emit event
        self.event_tx
            .send(PermissionEvent::PermissionRequired {
                session_id,
                request_id,
                request,
            })
            .await
            .map_err(|_| PermissionError::EventSendFailed)?;

        Ok(rx)
    }

    /// Responds to an individual permission request.
    ///
    /// # Arguments
    /// * `request_id` - ID of the request to respond to.
    /// * `granted` - Whether permission is granted.
    /// * `grant` - Optional grant to add if approved.
    ///
    /// # Returns
    /// Ok(()) if successful.
    pub async fn respond_to_request(
        &self,
        request_id: &str,
        granted: bool,
        grant: Option<Grant>,
    ) -> Result<(), PermissionError> {
        let pending = {
            let mut pending = self.pending_requests.lock().await;
            pending
                .remove(request_id)
                .ok_or(PermissionError::NotFound)?
        };

        // Add grant if provided and granted
        if granted {
            if let Some(g) = grant {
                self.add_grant(pending.session_id, g).await;
            }
        }

        pending
            .responder
            .send(granted)
            .map_err(|_| PermissionError::SendFailed)
    }

    // ========================================================================
    // Batch Permission Requests
    // ========================================================================

    /// Registers a batch of permission requests.
    ///
    /// Requests that are already satisfied by existing grants are auto-approved
    /// and not included in the batch sent to the UI.
    ///
    /// # Arguments
    /// * `session_id` - Session requesting permissions.
    /// * `requests` - The permission requests.
    ///
    /// # Returns
    /// A receiver that will receive the batch response.
    pub async fn register_batch(
        &self,
        session_id: i64,
        requests: Vec<PermissionRequest>,
    ) -> Result<oneshot::Receiver<BatchPermissionResponse>, PermissionError> {
        // Check which requests are already granted
        let auto_approved = self.check_batch(session_id, &requests).await;

        // Filter out auto-approved requests
        let needs_approval: Vec<_> = requests
            .iter()
            .filter(|r| !auto_approved.contains(&r.id))
            .cloned()
            .collect();

        // If all auto-approved, return immediately
        if needs_approval.is_empty() {
            let (tx, rx) = oneshot::channel();
            let response = BatchPermissionResponse::with_auto_approved(
                generate_batch_id(),
                auto_approved,
            );
            let _ = tx.send(response);
            return Ok(rx);
        }

        let batch_id = generate_batch_id();
        let (tx, rx) = oneshot::channel();

        // Create batch request with suggested grants
        let batch = BatchPermissionRequest::new(batch_id.clone(), needs_approval.clone());

        // Store pending batch
        {
            let mut pending = self.pending_batches.lock().await;
            pending.insert(
                batch_id.clone(),
                PendingBatch {
                    session_id,
                    requests: needs_approval,
                    responder: tx,
                },
            );
        }

        // Emit event
        self.event_tx
            .send(PermissionEvent::BatchPermissionRequired {
                session_id,
                batch,
            })
            .await
            .map_err(|_| PermissionError::EventSendFailed)?;

        Ok(rx)
    }

    /// Responds to a batch permission request.
    ///
    /// # Arguments
    /// * `batch_id` - ID of the batch to respond to.
    /// * `response` - The batch response with approved grants and denied requests.
    ///
    /// # Returns
    /// Ok(()) if successful.
    pub async fn respond_to_batch(
        &self,
        batch_id: &str,
        mut response: BatchPermissionResponse,
    ) -> Result<(), PermissionError> {
        let pending = {
            let mut pending = self.pending_batches.lock().await;
            pending
                .remove(batch_id)
                .ok_or(PermissionError::NotFound)?
        };

        // Add approved grants to session
        if !response.approved_grants.is_empty() {
            self.add_grants(pending.session_id, response.approved_grants.clone())
                .await;
        }

        // Ensure batch_id matches
        response.batch_id = batch_id.to_string();

        pending
            .responder
            .send(response)
            .map_err(|_| PermissionError::SendFailed)
    }

    // ========================================================================
    // Session Management
    // ========================================================================

    /// Cancels all pending requests for a session.
    ///
    /// This drops the response channels, causing receivers to get errors.
    ///
    /// # Arguments
    /// * `session_id` - Session to cancel.
    pub async fn cancel_session(&self, session_id: i64) {
        // Cancel individual requests
        {
            let mut pending = self.pending_requests.lock().await;
            pending.retain(|_, p| p.session_id != session_id);
        }

        // Cancel batch requests
        {
            let mut pending = self.pending_batches.lock().await;
            pending.retain(|_, p| p.session_id != session_id);
        }
    }

    /// Clears all state for a session (grants and pending requests).
    ///
    /// # Arguments
    /// * `session_id` - Session to clear.
    pub async fn clear_session(&self, session_id: i64) {
        self.cancel_session(session_id).await;
        self.clear_grants(session_id).await;
    }

    /// Checks if there are any pending requests for a session.
    ///
    /// # Arguments
    /// * `session_id` - Session to check.
    ///
    /// # Returns
    /// `true` if there are pending requests.
    pub async fn has_pending(&self, session_id: i64) -> bool {
        let individual_pending = {
            let pending = self.pending_requests.lock().await;
            pending.values().any(|p| p.session_id == session_id)
        };

        if individual_pending {
            return true;
        }

        let pending = self.pending_batches.lock().await;
        pending.values().any(|p| p.session_id == session_id)
    }

    /// Gets count of pending requests across all sessions.
    pub async fn pending_count(&self) -> usize {
        let individual = self.pending_requests.lock().await.len();
        let batch = self.pending_batches.lock().await.len();
        individual + batch
    }

    /// Gets pending request IDs for a session.
    pub async fn pending_request_ids(&self, session_id: i64) -> Vec<String> {
        let pending = self.pending_requests.lock().await;
        pending
            .iter()
            .filter(|(_, p)| p.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Gets pending batch IDs for a session.
    pub async fn pending_batch_ids(&self, session_id: i64) -> Vec<String> {
        let pending = self.pending_batches.lock().await;
        pending
            .iter()
            .filter(|(_, p)| p.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_read_request(id: &str, path: &str) -> PermissionRequest {
        PermissionRequest::file_read(id, path)
    }

    fn create_write_request(id: &str, path: &str) -> PermissionRequest {
        PermissionRequest::file_write(id, path)
    }

    #[tokio::test]
    async fn test_add_and_check_grant() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        let grant = Grant::read_path("/project/src", true);
        registry.add_grant(1, grant).await;

        let request = create_read_request("req-1", "/project/src/main.rs");
        assert!(registry.check(1, &request).await);

        // Different session should not have grant
        assert!(!registry.check(2, &request).await);
    }

    #[tokio::test]
    async fn test_level_hierarchy() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        // Add write grant
        let grant = Grant::write_path("/project", true);
        registry.add_grant(1, grant).await;

        // Read request should be satisfied (Write > Read)
        let read_request = create_read_request("req-1", "/project/file.rs");
        assert!(registry.check(1, &read_request).await);

        // Write request should also be satisfied
        let write_request = create_write_request("req-2", "/project/file.rs");
        assert!(registry.check(1, &write_request).await);
    }

    #[tokio::test]
    async fn test_level_hierarchy_insufficient() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        // Add read grant
        let grant = Grant::read_path("/project", true);
        registry.add_grant(1, grant).await;

        // Write request should NOT be satisfied (Read < Write)
        let write_request = create_write_request("req-1", "/project/file.rs");
        assert!(!registry.check(1, &write_request).await);
    }

    #[tokio::test]
    async fn test_recursive_path_grant() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        // Add recursive grant
        let grant = Grant::read_path("/project", true);
        registry.add_grant(1, grant).await;

        // Nested path should be covered
        let request = create_read_request("req-1", "/project/src/utils/mod.rs");
        assert!(registry.check(1, &request).await);
    }

    #[tokio::test]
    async fn test_non_recursive_path_grant() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        // Add non-recursive grant
        let grant = Grant::read_path("/project/src", false);
        registry.add_grant(1, grant).await;

        // Direct child should be covered
        let direct = create_read_request("req-1", "/project/src/main.rs");
        assert!(registry.check(1, &direct).await);

        // Nested path should NOT be covered
        let nested = create_read_request("req-2", "/project/src/utils/mod.rs");
        assert!(!registry.check(1, &nested).await);
    }

    #[tokio::test]
    async fn test_check_batch() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        // Add grant
        let grant = Grant::read_path("/project/src", true);
        registry.add_grant(1, grant).await;

        let requests = vec![
            create_read_request("req-1", "/project/src/main.rs"),
            create_read_request("req-2", "/project/tests/test.rs"), // Not covered
            create_read_request("req-3", "/project/src/lib.rs"),
        ];

        let granted = registry.check_batch(1, &requests).await;

        assert!(granted.contains("req-1"));
        assert!(!granted.contains("req-2"));
        assert!(granted.contains("req-3"));
    }

    #[tokio::test]
    async fn test_request_permission_auto_approve() {
        let (tx, mut rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        // Add grant first
        let grant = Grant::read_path("/project", true);
        registry.add_grant(1, grant).await;

        // Request should be auto-approved
        let request = create_read_request("req-1", "/project/file.rs");
        let result_rx = registry.request_permission(1, request).await.unwrap();

        // Should receive true immediately (auto-approved)
        let granted = result_rx.await.unwrap();
        assert!(granted);

        // No event should be emitted for auto-approved
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_request_permission_needs_approval() {
        let (tx, mut rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        // No grant - should need approval
        let request = create_read_request("req-1", "/project/file.rs");
        let result_rx = registry.request_permission(1, request).await.unwrap();

        // Event should be emitted
        let event = rx.recv().await.unwrap();
        if let PermissionEvent::PermissionRequired { request_id, .. } = event {
            assert_eq!(request_id, "req-1");
        } else {
            panic!("Expected PermissionRequired event");
        }

        // Respond to request
        let grant = Grant::read_path("/project", true);
        registry
            .respond_to_request("req-1", true, Some(grant))
            .await
            .unwrap();

        // Should receive approval
        let granted = result_rx.await.unwrap();
        assert!(granted);

        // Future requests should be auto-approved
        let new_request = create_read_request("req-2", "/project/other.rs");
        assert!(registry.check(1, &new_request).await);
    }

    #[tokio::test]
    async fn test_request_permission_denied() {
        let (tx, mut rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        let request = create_read_request("req-1", "/project/file.rs");
        let result_rx = registry.request_permission(1, request).await.unwrap();

        // Consume the event
        let _ = rx.recv().await.unwrap();

        // Deny the request
        registry
            .respond_to_request("req-1", false, None)
            .await
            .unwrap();

        // Should receive denial
        let granted = result_rx.await.unwrap();
        assert!(!granted);
    }

    #[tokio::test]
    async fn test_register_batch() {
        let (tx, mut rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        let requests = vec![
            create_read_request("req-1", "/project/src/main.rs"),
            create_read_request("req-2", "/project/src/lib.rs"),
        ];

        let result_rx = registry.register_batch(1, requests).await.unwrap();

        // Event should be emitted
        let event = rx.recv().await.unwrap();
        let batch_id = if let PermissionEvent::BatchPermissionRequired { batch, .. } = event {
            assert_eq!(batch.requests.len(), 2);
            assert!(!batch.suggested_grants.is_empty());
            batch.batch_id.clone()
        } else {
            panic!("Expected BatchPermissionRequired event");
        };

        // Respond with approval using the actual batch ID
        let grant = Grant::read_path("/project/src", true);
        let response = BatchPermissionResponse::all_granted(&batch_id, vec![grant]);
        registry.respond_to_batch(&batch_id, response).await.unwrap();

        // Should receive response
        let result = result_rx.await.unwrap();
        assert!(!result.approved_grants.is_empty());
    }

    #[tokio::test]
    async fn test_register_batch_partial_auto_approve() {
        let (tx, mut rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        // Add grant for /project/src
        let grant = Grant::read_path("/project/src", true);
        registry.add_grant(1, grant).await;

        let requests = vec![
            create_read_request("req-1", "/project/src/main.rs"), // Should be auto-approved
            create_read_request("req-2", "/project/tests/test.rs"), // Needs approval
        ];

        let result_rx = registry.register_batch(1, requests).await.unwrap();

        // Event should only contain non-auto-approved request
        let event = rx.recv().await.unwrap();
        let batch_id = if let PermissionEvent::BatchPermissionRequired { batch, .. } = event {
            assert_eq!(batch.requests.len(), 1);
            assert_eq!(batch.requests[0].id, "req-2");
            batch.batch_id.clone()
        } else {
            panic!("Expected BatchPermissionRequired event");
        };

        // Respond with approval for the remaining request
        let grant = Grant::read_path("/project/tests", true);
        let response = BatchPermissionResponse::all_granted(&batch_id, vec![grant]);
        registry.respond_to_batch(&batch_id, response).await.unwrap();

        let _ = result_rx.await.unwrap();
    }

    #[tokio::test]
    async fn test_register_batch_all_auto_approved() {
        let (tx, mut rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        // Add grant covering all requests
        let grant = Grant::read_path("/project", true);
        registry.add_grant(1, grant).await;

        let requests = vec![
            create_read_request("req-1", "/project/src/main.rs"),
            create_read_request("req-2", "/project/tests/test.rs"),
        ];

        let result_rx = registry.register_batch(1, requests).await.unwrap();

        // Should receive immediately with auto-approved
        let result = result_rx.await.unwrap();
        assert!(result.auto_approved.contains("req-1"));
        assert!(result.auto_approved.contains("req-2"));

        // No event should be emitted
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_revoke_grants() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        let grant1 = Grant::read_path("/project/src", true);
        let grant2 = Grant::read_path("/project/tests", true);
        registry.add_grant(1, grant1).await;
        registry.add_grant(1, grant2).await;

        // Revoke one grant
        let target = GrantTarget::path("/project/src", true);
        let revoked = registry.revoke_grants(1, &target).await;
        assert_eq!(revoked, 1);

        // First grant should be gone
        let request1 = create_read_request("req-1", "/project/src/file.rs");
        assert!(!registry.check(1, &request1).await);

        // Second grant should remain
        let request2 = create_read_request("req-2", "/project/tests/test.rs");
        assert!(registry.check(1, &request2).await);
    }

    #[tokio::test]
    async fn test_clear_session() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        let grant = Grant::read_path("/project", true);
        registry.add_grant(1, grant).await;

        // Clear session
        registry.clear_session(1).await;

        // Grant should be gone
        let request = create_read_request("req-1", "/project/file.rs");
        assert!(!registry.check(1, &request).await);
    }

    #[tokio::test]
    async fn test_cancel_session() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        // Register a pending request
        let request = create_read_request("req-1", "/project/file.rs");
        let result_rx = registry.request_permission(1, request).await.unwrap();

        assert!(registry.has_pending(1).await);

        // Cancel session
        registry.cancel_session(1).await;

        // Should no longer have pending
        assert!(!registry.has_pending(1).await);

        // Receiver should get error (channel dropped)
        assert!(result_rx.await.is_err());
    }

    #[tokio::test]
    async fn test_domain_grant() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        let grant = Grant::domain("*.github.com", PermissionLevel::Read);
        registry.add_grant(1, grant).await;

        let request =
            PermissionRequest::network_access("req-1", "api.github.com", PermissionLevel::Read);
        assert!(registry.check(1, &request).await);

        let other_domain =
            PermissionRequest::network_access("req-2", "api.gitlab.com", PermissionLevel::Read);
        assert!(!registry.check(1, &other_domain).await);
    }

    #[tokio::test]
    async fn test_command_grant() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        let grant = Grant::command("git *", PermissionLevel::Execute);
        registry.add_grant(1, grant).await;

        let request = PermissionRequest::command_execute("req-1", "git status");
        assert!(registry.check(1, &request).await);

        let other_cmd = PermissionRequest::command_execute("req-2", "docker run nginx");
        assert!(!registry.check(1, &other_cmd).await);
    }

    #[tokio::test]
    async fn test_find_satisfying_grant() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        let grant = Grant::write_path("/project", true);
        registry.add_grant(1, grant.clone()).await;

        let request = create_read_request("req-1", "/project/file.rs");
        let found = registry.find_satisfying_grant(1, &request).await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().target, grant.target);
    }

    #[tokio::test]
    async fn test_pending_counts() {
        let (tx, _rx) = mpsc::channel(10);
        let registry = PermissionRegistry::new(tx);

        assert_eq!(registry.pending_count().await, 0);

        let request = create_read_request("req-1", "/project/file.rs");
        let _ = registry.request_permission(1, request).await;

        assert_eq!(registry.pending_count().await, 1);

        let ids = registry.pending_request_ids(1).await;
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], "req-1");
    }
}
