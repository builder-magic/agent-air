use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::registry::ToolRegistry;
use super::types::{ToolBatchResult, ToolContext, ToolRequest, ToolResult};
use crate::controller::types::TurnId;
use crate::permissions::{PermissionRegistry, PermissionRequest};

/// Manages tool execution with support for parallel batch execution.
pub struct ToolExecutor {
    registry: Arc<ToolRegistry>,
    permission_registry: Arc<PermissionRegistry>,
    tool_result_tx: mpsc::Sender<ToolResult>,
    batch_result_tx: mpsc::Sender<ToolBatchResult>,
    batch_counter: AtomicI64,
}

impl ToolExecutor {
    /// Create a new tool executor.
    ///
    /// # Arguments
    /// * `registry` - Tool registry for looking up tools
    /// * `permission_registry` - Permission registry for batch permission requests
    /// * `tool_result_tx` - Channel for individual tool results (UI feedback)
    /// * `batch_result_tx` - Channel for batch results (sending to LLM)
    pub fn new(
        registry: Arc<ToolRegistry>,
        permission_registry: Arc<PermissionRegistry>,
        tool_result_tx: mpsc::Sender<ToolResult>,
        batch_result_tx: mpsc::Sender<ToolBatchResult>,
    ) -> Self {
        Self {
            registry,
            permission_registry,
            tool_result_tx,
            batch_result_tx,
            batch_counter: AtomicI64::new(0),
        }
    }

    /// Execute a batch of tools in parallel.
    ///
    /// This method implements batch permission handling:
    /// 1. Collects required permissions from all tools via `required_permissions()`
    /// 2. Requests batch approval from the permission registry (single UI prompt)
    /// 3. If approved: executes all tools with `permissions_pre_approved: true`
    /// 4. If denied: returns error results for all tools
    ///
    /// Tools that handle their own permissions (`handles_own_permissions() -> true`)
    /// are always executed regardless of batch permission status.
    ///
    /// Results are emitted individually to tool_result_tx for UI feedback,
    /// and the complete batch is sent to batch_result_tx when all tools finish.
    ///
    /// Returns the batch ID.
    pub async fn execute_batch(
        &self,
        session_id: i64,
        turn_id: Option<TurnId>,
        requests: Vec<ToolRequest>,
        cancel_token: CancellationToken,
    ) -> i64 {
        let batch_id = self.batch_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let expected_count = requests.len();

        if expected_count == 0 {
            // Empty batch - send empty result immediately
            let batch_result = ToolBatchResult {
                batch_id,
                session_id,
                turn_id,
                results: Vec::new(),
            };
            if let Err(e) = self.batch_result_tx.send(batch_result).await {
                tracing::debug!("Failed to send empty batch result: {}", e);
            }
            return batch_id;
        }

        tracing::debug!(
            batch_id,
            session_id,
            tool_count = expected_count,
            "Starting tool batch execution"
        );

        // Collect permission requirements from all tools
        let mut all_permissions: Vec<PermissionRequest> = Vec::new();
        let mut tools_needing_permissions: Vec<String> = Vec::new();

        for request in &requests {
            if let Some(tool) = self.registry.get(&request.tool_name).await {
                // Skip tools that handle their own permissions
                if tool.handles_own_permissions() {
                    continue;
                }

                // Build context for permission check
                let context = ToolContext::new(session_id, &request.tool_use_id, turn_id.clone());

                // Collect required permissions
                if let Some(perms) = tool.required_permissions(&context, &request.input) {
                    if !perms.is_empty() {
                        tools_needing_permissions.push(request.tool_use_id.clone());
                        all_permissions.extend(perms);
                    }
                }
            }
        }

        // Determine if batch permissions are pre-approved
        let permissions_pre_approved = if !all_permissions.is_empty() {
            tracing::debug!(
                batch_id,
                permission_count = all_permissions.len(),
                tool_count = tools_needing_permissions.len(),
                "Requesting permissions"
            );

            // Use single permission request for single permission, batch for multiple
            if all_permissions.len() == 1 {
                // Single permission - use simpler PermissionPanel UI
                let permission = all_permissions.into_iter().next().unwrap();
                match self
                    .permission_registry
                    .request_permission(session_id, permission, turn_id.clone())
                    .await
                {
                    Ok(rx) => {
                        match rx.await {
                            Ok(response) => {
                                if response.granted {
                                    tracing::info!(batch_id, "Single permission approved");
                                    true
                                } else {
                                    tracing::info!(batch_id, "Single permission denied");

                                    // Create error results for all tools
                                    let error_results: Vec<ToolResult> = requests
                                        .iter()
                                        .map(|req| {
                                            ToolResult::error(
                                                session_id,
                                                req.tool_name.clone(),
                                                req.tool_use_id.clone(),
                                                req.input.clone(),
                                                "Permission denied by user".to_string(),
                                                turn_id.clone(),
                                            )
                                        })
                                        .collect();

                                    // Send individual error results
                                    for result in &error_results {
                                        if let Err(e) =
                                            self.tool_result_tx.send(result.clone()).await
                                        {
                                            tracing::debug!("Failed to send tool result: {}", e);
                                        }
                                    }

                                    // Send batch result
                                    let batch_result = ToolBatchResult {
                                        batch_id,
                                        session_id,
                                        turn_id,
                                        results: error_results,
                                    };
                                    if let Err(e) = self.batch_result_tx.send(batch_result).await {
                                        tracing::debug!("Failed to send batch result: {}", e);
                                    }

                                    return batch_id;
                                }
                            }
                            Err(_) => {
                                // Channel closed - permission request was cancelled
                                tracing::info!(batch_id, "Single permission request cancelled");

                                let error_results: Vec<ToolResult> = requests
                                    .iter()
                                    .map(|req| {
                                        ToolResult::error(
                                            session_id,
                                            req.tool_name.clone(),
                                            req.tool_use_id.clone(),
                                            req.input.clone(),
                                            "Permission request cancelled".to_string(),
                                            turn_id.clone(),
                                        )
                                    })
                                    .collect();

                                for result in &error_results {
                                    if let Err(e) = self.tool_result_tx.send(result.clone()).await {
                                        tracing::debug!("Failed to send tool result: {}", e);
                                    }
                                }

                                let batch_result = ToolBatchResult {
                                    batch_id,
                                    session_id,
                                    turn_id,
                                    results: error_results,
                                };
                                if let Err(e) = self.batch_result_tx.send(batch_result).await {
                                    tracing::debug!("Failed to send batch result: {}", e);
                                }

                                return batch_id;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(batch_id, error = %e, "Failed to request single permission");

                        let error_results: Vec<ToolResult> = requests
                            .iter()
                            .map(|req| {
                                ToolResult::error(
                                    session_id,
                                    req.tool_name.clone(),
                                    req.tool_use_id.clone(),
                                    req.input.clone(),
                                    format!("Permission request failed: {}", e),
                                    turn_id.clone(),
                                )
                            })
                            .collect();

                        for result in &error_results {
                            if let Err(e) = self.tool_result_tx.send(result.clone()).await {
                                tracing::debug!("Failed to send tool result: {}", e);
                            }
                        }

                        let batch_result = ToolBatchResult {
                            batch_id,
                            session_id,
                            turn_id,
                            results: error_results,
                        };
                        if let Err(e) = self.batch_result_tx.send(batch_result).await {
                            tracing::debug!("Failed to send batch result: {}", e);
                        }

                        return batch_id;
                    }
                }
            } else {
                // Multiple permissions - use BatchPermissionPanel UI
                match self
                    .permission_registry
                    .register_batch(session_id, all_permissions, turn_id.clone())
                    .await
                {
                Ok(rx) => {
                    // Wait for permission response
                    match rx.await {
                        Ok(response) => {
                            // Batch permissions: all-or-none model
                            // If any requests were denied, fail all tools
                            if !response.denied_requests.is_empty() {
                                tracing::info!(
                                    batch_id,
                                    denied_count = response.denied_requests.len(),
                                    "Batch permissions denied"
                                );

                                // Create error results for all tools
                                let error_results: Vec<ToolResult> = requests
                                    .iter()
                                    .map(|req| {
                                        ToolResult::error(
                                            session_id,
                                            req.tool_name.clone(),
                                            req.tool_use_id.clone(),
                                            req.input.clone(),
                                            "Permission denied by user".to_string(),
                                            turn_id.clone(),
                                        )
                                    })
                                    .collect();

                                // Send individual error results
                                for result in &error_results {
                                    if let Err(e) =
                                        self.tool_result_tx.send(result.clone()).await
                                    {
                                        tracing::debug!("Failed to send tool result: {}", e);
                                    }
                                }

                                // Send batch result
                                let batch_result = ToolBatchResult {
                                    batch_id,
                                    session_id,
                                    turn_id,
                                    results: error_results,
                                };
                                if let Err(e) = self.batch_result_tx.send(batch_result).await {
                                    tracing::debug!("Failed to send batch result: {}", e);
                                }

                                return batch_id;
                            }

                            tracing::info!(
                                batch_id,
                                grant_count = response.approved_grants.len(),
                                "Batch permissions approved"
                            );
                            true
                        }
                        Err(_) => {
                            // Channel closed - permission request was cancelled
                            tracing::info!(batch_id, "Batch permission request cancelled");

                            // Create error results for all tools
                            let error_results: Vec<ToolResult> = requests
                                .iter()
                                .map(|req| {
                                    ToolResult::error(
                                        session_id,
                                        req.tool_name.clone(),
                                        req.tool_use_id.clone(),
                                        req.input.clone(),
                                        "Permission request cancelled".to_string(),
                                        turn_id.clone(),
                                    )
                                })
                                .collect();

                            // Send individual error results
                            for result in &error_results {
                                if let Err(e) = self.tool_result_tx.send(result.clone()).await {
                                    tracing::debug!("Failed to send tool result: {}", e);
                                }
                            }

                            // Send batch result
                            let batch_result = ToolBatchResult {
                                batch_id,
                                session_id,
                                turn_id,
                                results: error_results,
                            };
                            if let Err(e) = self.batch_result_tx.send(batch_result).await {
                                tracing::debug!("Failed to send batch result: {}", e);
                            }

                            return batch_id;
                        }
                    }
                }
                Err(e) => {
                    // Failed to register batch - treat as permission denied
                    tracing::warn!(
                        batch_id,
                        error = %e,
                        "Failed to register batch permission request"
                    );
                    false
                }
                }
            }
        } else {
            // No permissions needed
            true
        };

        // Create batch state
        let batch = Arc::new(ToolExecutorBatch {
            batch_id,
            session_id,
            turn_id: turn_id.clone(),
            tool_result_tx: self.tool_result_tx.clone(),
            batch_result_tx: self.batch_result_tx.clone(),
            requests: requests.clone(),
            results: Mutex::new(HashMap::new()),
            expected_count,
            permissions_pre_approved,
            task_handles: Mutex::new(Vec::with_capacity(expected_count)),
        });

        // Start all tools concurrently, storing JoinHandles for tracking
        for request in requests {
            let batch_clone = batch.clone();
            let registry = self.registry.clone();
            let cancel = cancel_token.clone();
            let turn_id = turn_id.clone();

            let handle = tokio::spawn(async move {
                batch_clone
                    .run_tool(registry, request, turn_id, cancel)
                    .await;
            });

            // Store the handle for tracking
            batch.task_handles.lock().await.push(handle);
        }

        batch_id
    }

    /// Execute a single tool (convenience method that creates a batch of 1).
    pub async fn execute(
        &self,
        session_id: i64,
        turn_id: Option<TurnId>,
        request: ToolRequest,
        cancel_token: CancellationToken,
    ) -> i64 {
        self.execute_batch(session_id, turn_id, vec![request], cancel_token)
            .await
    }
}

/// Internal batch state for tracking parallel tool executions.
struct ToolExecutorBatch {
    batch_id: i64,
    session_id: i64,
    turn_id: Option<TurnId>,
    tool_result_tx: mpsc::Sender<ToolResult>,
    batch_result_tx: mpsc::Sender<ToolBatchResult>,
    requests: Vec<ToolRequest>,
    results: Mutex<HashMap<String, ToolResult>>,
    expected_count: usize,
    /// Whether permissions were pre-approved by the batch executor.
    permissions_pre_approved: bool,
    /// JoinHandles for spawned tool tasks, enabling graceful shutdown and panic detection.
    task_handles: Mutex<Vec<JoinHandle<()>>>,
}

impl ToolExecutorBatch {
    /// Run a single tool and add result to the batch.
    async fn run_tool(
        &self,
        registry: Arc<ToolRegistry>,
        request: ToolRequest,
        turn_id: Option<TurnId>,
        cancel_token: CancellationToken,
    ) {
        let tool_use_id = request.tool_use_id.clone();
        let tool_name = request.tool_name.clone();
        let input = request.input.clone();

        tracing::debug!(
            batch_id = self.batch_id,
            session_id = self.session_id,
            tool_name = %tool_name,
            tool_use_id = %tool_use_id,
            "Starting tool execution"
        );

        // Look up tool in registry
        let tool = registry.get(&tool_name).await;

        let result = match tool {
            None => {
                // Tool not found
                tracing::warn!(
                    batch_id = self.batch_id,
                    tool_name = %tool_name,
                    "Tool not found in registry"
                );
                ToolResult::error(
                    self.session_id,
                    tool_name,
                    tool_use_id,
                    input,
                    format!("Tool not found: {}", request.tool_name),
                    turn_id,
                )
            }
            Some(tool) => {
                // Get display name from tool's display config
                let display_name = Some(tool.display_config().display_name);

                // Build tool context with pre-approved flag from batch
                let context = ToolContext {
                    session_id: self.session_id,
                    tool_use_id: tool_use_id.clone(),
                    turn_id: turn_id.clone(),
                    permissions_pre_approved: self.permissions_pre_approved,
                };

                // Execute tool with cancellation support
                tokio::select! {
                    exec_result = tool.execute(context, input.clone()) => {
                        match exec_result {
                            Ok(content) => {
                                tracing::info!(
                                    batch_id = self.batch_id,
                                    tool_name = %tool_name,
                                    result_bytes = content.len(),
                                    "Tool execution succeeded"
                                );
                                // Compute compact summary for compaction
                                let compact_summary = Some(tool.compact_summary(&input, &content));
                                ToolResult::success(
                                    self.session_id,
                                    tool_name,
                                    display_name,
                                    tool_use_id,
                                    input,
                                    content,
                                    turn_id,
                                    compact_summary,
                                )
                            }
                            Err(error) => {
                                tracing::warn!(
                                    batch_id = self.batch_id,
                                    tool_name = %tool_name,
                                    error = %error,
                                    "Tool execution failed"
                                );
                                ToolResult::error(
                                    self.session_id,
                                    tool_name,
                                    tool_use_id,
                                    input,
                                    error,
                                    turn_id,
                                )
                            }
                        }
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::warn!(
                            batch_id = self.batch_id,
                            tool_name = %tool_name,
                            "Tool execution cancelled"
                        );
                        ToolResult::timeout(
                            self.session_id,
                            tool_name,
                            tool_use_id,
                            input,
                            turn_id,
                        )
                    }
                }
            }
        };

        self.add_result(result).await;
    }

    /// Add a result to the batch and check for completion.
    async fn add_result(&self, result: ToolResult) {
        // Send individual result for UI feedback
        if let Err(e) = self.tool_result_tx.send(result.clone()).await {
            tracing::debug!("Failed to send tool result: {}", e);
        }

        let mut results = self.results.lock().await;
        results.insert(result.tool_use_id.clone(), result);

        tracing::debug!(
            batch_id = self.batch_id,
            completed = results.len(),
            expected = self.expected_count,
            "Tool completed in batch"
        );

        // Check if all tools have completed
        if results.len() == self.expected_count {
            self.send_batch_result(&results).await;
        }
    }

    /// Send the complete batch result.
    async fn send_batch_result(&self, results: &HashMap<String, ToolResult>) {
        // Build results in original request order
        let ordered_results: Vec<ToolResult> = self
            .requests
            .iter()
            .filter_map(|req| results.get(&req.tool_use_id).cloned())
            .collect();

        let batch_result = ToolBatchResult {
            batch_id: self.batch_id,
            session_id: self.session_id,
            turn_id: self.turn_id.clone(),
            results: ordered_results,
        };

        tracing::debug!(
            batch_id = self.batch_id,
            session_id = self.session_id,
            result_count = batch_result.results.len(),
            "Sending batch result"
        );

        if let Err(e) = self.batch_result_tx.send(batch_result).await {
            tracing::debug!("Failed to send batch result: {}", e);
        }
    }

    /// Await completion of all spawned tasks with an optional timeout.
    ///
    /// This method drains the task handles and awaits each one. If a timeout is provided,
    /// tasks that don't complete within the timeout will be logged but not forcefully aborted
    /// (Tokio tasks can only be aborted by dropping the handle, which we do after timeout).
    ///
    /// # Arguments
    /// * `timeout` - Optional timeout duration. If None, waits indefinitely.
    ///
    /// # Returns
    /// A tuple of (completed_count, panicked_count, timed_out_count)
    #[allow(dead_code)] // Available for future use in graceful shutdown
    async fn await_completion(&self, timeout: Option<Duration>) -> (usize, usize, usize) {
        let handles: Vec<JoinHandle<()>> = {
            let mut guard = self.task_handles.lock().await;
            std::mem::take(&mut *guard)
        };

        let total = handles.len();
        let mut completed = 0;
        let mut panicked = 0;
        let mut timed_out = 0;

        for handle in handles {
            let result = if let Some(timeout_duration) = timeout {
                match tokio::time::timeout(timeout_duration, handle).await {
                    Ok(join_result) => Some(join_result),
                    Err(_) => {
                        timed_out += 1;
                        tracing::warn!(
                            batch_id = self.batch_id,
                            "Task did not complete within timeout"
                        );
                        None
                    }
                }
            } else {
                Some(handle.await)
            };

            if let Some(join_result) = result {
                match join_result {
                    Ok(()) => completed += 1,
                    Err(e) => {
                        panicked += 1;
                        if e.is_panic() {
                            tracing::error!(
                                batch_id = self.batch_id,
                                error = %e,
                                "Task panicked"
                            );
                        } else {
                            tracing::warn!(
                                batch_id = self.batch_id,
                                error = %e,
                                "Task was cancelled"
                            );
                        }
                    }
                }
            }
        }

        tracing::debug!(
            batch_id = self.batch_id,
            total,
            completed,
            panicked,
            timed_out,
            "Batch task completion summary"
        );

        (completed, panicked, timed_out)
    }

    /// Returns the number of tasks that are still running.
    #[allow(dead_code)] // Available for future use in monitoring
    async fn active_task_count(&self) -> usize {
        let handles = self.task_handles.lock().await;
        handles.iter().filter(|h| !h.is_finished()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::tools::types::{Executable, ToolResultStatus, ToolType};
    use crate::controller::types::ControllerEvent;
    use std::future::Future;
    use std::pin::Pin;
    use std::time::Duration;

    /// Create a test permission registry that auto-approves everything.
    fn create_test_permission_registry() -> Arc<PermissionRegistry> {
        let (event_tx, _event_rx) = mpsc::channel::<ControllerEvent>(10);
        Arc::new(PermissionRegistry::new(event_tx))
    }

    struct EchoTool;

    impl Executable for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echoes input back"
        }

        fn input_schema(&self) -> &str {
            r#"{"type":"object","properties":{"message":{"type":"string"}}}"#
        }

        fn tool_type(&self) -> ToolType {
            ToolType::Custom
        }

        fn execute(
            &self,
            _context: ToolContext,
            input: HashMap<String, serde_json::Value>,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            let message = input
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("no message")
                .to_string();
            Box::pin(async move { Ok(format!("Echo: {}", message)) })
        }
    }

    struct SlowTool;

    impl Executable for SlowTool {
        fn name(&self) -> &str {
            "slow"
        }

        fn description(&self) -> &str {
            "A slow tool for testing timeouts"
        }

        fn input_schema(&self) -> &str {
            r#"{"type":"object"}"#
        }

        fn tool_type(&self) -> ToolType {
            ToolType::Custom
        }

        fn execute(
            &self,
            _context: ToolContext,
            _input: HashMap<String, serde_json::Value>,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok("done".to_string())
            })
        }
    }

    #[tokio::test]
    async fn test_execute_single_tool() {
        let registry = Arc::new(ToolRegistry::new());
        registry.register(Arc::new(EchoTool)).await.unwrap();

        let permission_registry = create_test_permission_registry();
        let (tool_tx, mut tool_rx) = mpsc::channel(10);
        let (batch_tx, mut batch_rx) = mpsc::channel(10);

        let executor = ToolExecutor::new(registry, permission_registry, tool_tx, batch_tx);

        let mut input = HashMap::new();
        input.insert(
            "message".to_string(),
            serde_json::Value::String("hello".to_string()),
        );

        let request = ToolRequest {
            tool_use_id: "test_1".to_string(),
            tool_name: "echo".to_string(),
            input,
        };

        let cancel = CancellationToken::new();
        executor.execute(1, None, request, cancel).await;

        // Wait for individual result
        let result = tool_rx.recv().await.unwrap();
        assert_eq!(result.status, ToolResultStatus::Success);
        assert!(result.content.contains("Echo: hello"));

        // Wait for batch result
        let batch = batch_rx.recv().await.unwrap();
        assert_eq!(batch.results.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_batch() {
        let registry = Arc::new(ToolRegistry::new());
        registry.register(Arc::new(EchoTool)).await.unwrap();

        let permission_registry = create_test_permission_registry();
        let (tool_tx, mut tool_rx) = mpsc::channel(10);
        let (batch_tx, mut batch_rx) = mpsc::channel(10);

        let executor = ToolExecutor::new(registry, permission_registry, tool_tx, batch_tx);

        let requests: Vec<ToolRequest> = (0..3)
            .map(|i| {
                let mut input = HashMap::new();
                input.insert(
                    "message".to_string(),
                    serde_json::Value::String(format!("msg_{}", i)),
                );
                ToolRequest {
                    tool_use_id: format!("tool_{}", i),
                    tool_name: "echo".to_string(),
                    input,
                }
            })
            .collect();

        let cancel = CancellationToken::new();
        executor.execute_batch(1, None, requests, cancel).await;

        // Collect individual results
        for _ in 0..3 {
            let result = tool_rx.recv().await.unwrap();
            assert_eq!(result.status, ToolResultStatus::Success);
        }

        // Wait for batch result
        let batch = batch_rx.recv().await.unwrap();
        assert_eq!(batch.results.len(), 3);
    }

    #[tokio::test]
    async fn test_tool_not_found() {
        let registry = Arc::new(ToolRegistry::new());

        let permission_registry = create_test_permission_registry();
        let (tool_tx, mut tool_rx) = mpsc::channel(10);
        let (batch_tx, _batch_rx) = mpsc::channel(10);

        let executor = ToolExecutor::new(registry, permission_registry, tool_tx, batch_tx);

        let request = ToolRequest {
            tool_use_id: "test_1".to_string(),
            tool_name: "nonexistent".to_string(),
            input: HashMap::new(),
        };

        let cancel = CancellationToken::new();
        executor.execute(1, None, request, cancel).await;

        let result = tool_rx.recv().await.unwrap();
        assert_eq!(result.status, ToolResultStatus::Error);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_tool_cancellation() {
        let registry = Arc::new(ToolRegistry::new());
        registry.register(Arc::new(SlowTool)).await.unwrap();

        let permission_registry = create_test_permission_registry();
        let (tool_tx, mut tool_rx) = mpsc::channel(10);
        let (batch_tx, _batch_rx) = mpsc::channel(10);

        let executor = ToolExecutor::new(registry, permission_registry, tool_tx, batch_tx);

        let request = ToolRequest {
            tool_use_id: "test_1".to_string(),
            tool_name: "slow".to_string(),
            input: HashMap::new(),
        };

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        // Start execution
        executor.execute(1, None, request, cancel).await;

        // Cancel after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel_clone.cancel();
        });

        // Wait for result - should be timeout/cancelled
        let result = tool_rx.recv().await.unwrap();
        assert_eq!(result.status, ToolResultStatus::Timeout);
    }
}
