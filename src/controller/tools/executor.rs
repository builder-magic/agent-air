use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use super::registry::ToolRegistry;
use super::types::{ToolBatchResult, ToolContext, ToolRequest, ToolResult};
use crate::controller::types::TurnId;

/// Manages tool execution with support for parallel batch execution.
pub struct ToolExecutor {
    registry: Arc<ToolRegistry>,
    tool_result_tx: mpsc::Sender<ToolResult>,
    batch_result_tx: mpsc::Sender<ToolBatchResult>,
    batch_counter: AtomicI64,
}

impl ToolExecutor {
    /// Create a new tool executor.
    ///
    /// # Arguments
    /// * `registry` - Tool registry for looking up tools
    /// * `tool_result_tx` - Channel for individual tool results (UI feedback)
    /// * `batch_result_tx` - Channel for batch results (sending to LLM)
    pub fn new(
        registry: Arc<ToolRegistry>,
        tool_result_tx: mpsc::Sender<ToolResult>,
        batch_result_tx: mpsc::Sender<ToolBatchResult>,
    ) -> Self {
        Self {
            registry,
            tool_result_tx,
            batch_result_tx,
            batch_counter: AtomicI64::new(0),
        }
    }

    /// Execute a batch of tools in parallel.
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
        });

        // Start all tools concurrently
        for request in requests {
            let batch = batch.clone();
            let registry = self.registry.clone();
            let cancel = cancel_token.clone();
            let turn_id = turn_id.clone();

            tokio::spawn(async move {
                batch
                    .run_tool(registry, request, turn_id, cancel)
                    .await;
            });
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

                // Build tool context
                let context = ToolContext {
                    session_id: self.session_id,
                    tool_use_id: tool_use_id.clone(),
                    turn_id: turn_id.clone(),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::tools::types::{Executable, ToolResultStatus, ToolType};
    use std::future::Future;
    use std::pin::Pin;
    use std::time::Duration;

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

        let (tool_tx, mut tool_rx) = mpsc::channel(10);
        let (batch_tx, mut batch_rx) = mpsc::channel(10);

        let executor = ToolExecutor::new(registry, tool_tx, batch_tx);

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

        let (tool_tx, mut tool_rx) = mpsc::channel(10);
        let (batch_tx, mut batch_rx) = mpsc::channel(10);

        let executor = ToolExecutor::new(registry, tool_tx, batch_tx);

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

        let (tool_tx, mut tool_rx) = mpsc::channel(10);
        let (batch_tx, _batch_rx) = mpsc::channel(10);

        let executor = ToolExecutor::new(registry, tool_tx, batch_tx);

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

        let (tool_tx, mut tool_rx) = mpsc::channel(10);
        let (batch_tx, _batch_rx) = mpsc::channel(10);

        let executor = ToolExecutor::new(registry, tool_tx, batch_tx);

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
