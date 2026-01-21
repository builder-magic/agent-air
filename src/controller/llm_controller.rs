// This implements the main LLM controller - the entry point of the library

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use std::sync::Arc;

use crate::client::error::LlmError;
use crate::controller::session::{LLMSession, LLMSessionConfig, LLMSessionManager};
use crate::controller::tools::{
    AskUserQuestionsResponse, PendingPermissionInfo, PendingQuestionInfo, PermissionError,
    PermissionRegistry, PermissionResponse, ToolBatchResult, ToolExecutor, ToolRegistry,
    ToolRequest, ToolResult, UserInteractionError, UserInteractionRegistry,
};
use crate::controller::types::{
    ControlCmd, ControllerEvent, ControllerInputPayload, FromLLMPayload, InputType,
    LLMRequestType, LLMResponseType, ToLLMPayload, TurnId,
};
use crate::controller::usage::TokenUsageTracker;

/// Callback function type for controller events
pub type EventFunc = Box<dyn Fn(ControllerEvent) + Send + Sync>;

/// Channel buffer size for internal communication
const CHANNEL_BUFFER_SIZE: usize = 100;

/// Timeout for sending input to the controller
const SEND_INPUT_TIMEOUT: Duration = Duration::from_secs(5);

/// The main LLM controller that manages sessions and coordinates communication
pub struct LLMController {
    /// Session manager for creating and managing LLM sessions
    session_mgr: LLMSessionManager,

    /// Token usage tracker
    token_usage: TokenUsageTracker,

    /// Receiver for responses from LLM sessions
    from_llm_rx: Mutex<mpsc::Receiver<FromLLMPayload>>,

    /// Sender for responses from LLM sessions (passed to new sessions)
    from_llm_tx: mpsc::Sender<FromLLMPayload>,

    /// Receiver for user input and control commands
    input_rx: Mutex<mpsc::Receiver<ControllerInputPayload>>,

    /// Sender for user input (used by send_input)
    input_tx: mpsc::Sender<ControllerInputPayload>,

    /// Whether the controller has been started
    started: AtomicBool,

    /// Whether the controller has been shutdown
    shutdown: AtomicBool,

    /// Cancellation token for graceful shutdown
    cancel_token: CancellationToken,

    /// Optional callback for external event consumers
    event_func: Option<EventFunc>,

    /// Tool registry for managing available tools
    tool_registry: Arc<ToolRegistry>,

    /// Tool executor for running tools
    tool_executor: ToolExecutor,

    /// Receiver for individual tool results (for UI feedback)
    tool_result_rx: Mutex<mpsc::Receiver<ToolResult>>,

    /// Receiver for batch tool results (for sending to LLM)
    batch_result_rx: Mutex<mpsc::Receiver<ToolBatchResult>>,

    /// Registry for user interaction tools
    user_interaction_registry: Arc<UserInteractionRegistry>,

    /// Receiver for user interaction events
    user_interaction_rx: Mutex<mpsc::Receiver<ControllerEvent>>,

    /// Registry for permission requests
    permission_registry: Arc<PermissionRegistry>,

    /// Receiver for permission events
    permission_rx: Mutex<mpsc::Receiver<ControllerEvent>>,
}

impl LLMController {
    /// Creates a new LLM controller
    ///
    /// # Arguments
    /// * `event_func` - Optional callback for controller events
    pub fn new(event_func: Option<EventFunc>) -> Self {
        let (from_llm_tx, from_llm_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
        let (input_tx, input_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);

        // Create tool execution channels
        let (tool_result_tx, tool_result_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
        let (batch_result_tx, batch_result_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);

        // Create user interaction registry with event channel
        let (user_interaction_tx, user_interaction_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
        let user_interaction_registry = Arc::new(UserInteractionRegistry::new(user_interaction_tx));

        // Create permission registry with event channel
        let (permission_tx, permission_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
        let permission_registry = Arc::new(PermissionRegistry::new(permission_tx));

        let tool_registry = Arc::new(ToolRegistry::new());
        let tool_executor = ToolExecutor::new(
            tool_registry.clone(),
            tool_result_tx,
            batch_result_tx,
        );

        Self {
            session_mgr: LLMSessionManager::new(),
            token_usage: TokenUsageTracker::new(),
            from_llm_rx: Mutex::new(from_llm_rx),
            from_llm_tx,
            input_rx: Mutex::new(input_rx),
            input_tx,
            started: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            cancel_token: CancellationToken::new(),
            event_func,
            tool_registry,
            tool_executor,
            tool_result_rx: Mutex::new(tool_result_rx),
            batch_result_rx: Mutex::new(batch_result_rx),
            user_interaction_registry,
            user_interaction_rx: Mutex::new(user_interaction_rx),
            permission_registry,
            permission_rx: Mutex::new(permission_rx),
        }
    }

    /// Starts the controller's event loop.
    /// This is idempotent - calling it multiple times has no effect.
    /// Should be spawned as a tokio task.
    pub async fn start(&self) {
        // Ensure we only start once
        if self
            .started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            tracing::warn!("Controller already started");
            return;
        }

        tracing::info!("Controller starting");

        // Main event loop - processes events from 6 channels using tokio::select!
        //
        // DESIGN NOTE: Mutex Pattern for Multiple Receivers
        // -------------------------------------------------
        // This loop acquires all 6 receiver locks at the start of each iteration,
        // then immediately drops them in each select! branch. This pattern is:
        //
        // 1. SAFE: No deadlock risk - locks acquired in consistent order, released immediately
        // 2. EFFICIENT: Locks held only during polling (~microseconds), not while waiting
        // 3. NON-BLOCKING: Tokio's mpsc senders are lock-free; only receivers need mutex
        // 4. CLEAR: Explicit drops make guard lifecycle obvious
        //
        // Alternative patterns considered:
        // - Unified event channel: Would lose type safety, require boxing all events
        // - Select on lock().recv() directly: Makes code harder to reason about
        // - Lock-free structures: Overkill; Tokio primitives are already optimized
        //
        // The receivers are only accessed here; senders are distributed to:
        // - from_llm_tx: LLM session tasks (streaming responses)
        // - input_tx: UI thread (user input with timeout)
        // - batch_result_tx: Tool executor (completed batches)
        // - tool_result_tx: Individual tool tasks (UI feedback)
        // - user_interaction_tx: UserInteractionRegistry (tool questions)
        // - permission_tx: PermissionRegistry (permission requests)
        loop {
            let mut from_llm_guard = self.from_llm_rx.lock().await;
            let mut input_guard = self.input_rx.lock().await;
            let mut batch_result_guard = self.batch_result_rx.lock().await;
            let mut tool_result_guard = self.tool_result_rx.lock().await;
            let mut user_interaction_guard = self.user_interaction_rx.lock().await;
            let mut permission_guard = self.permission_rx.lock().await;

            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    tracing::info!("Controller cancelled");
                    break;
                }
                msg = from_llm_guard.recv() => {
                    drop(from_llm_guard);
                    drop(input_guard);
                    drop(batch_result_guard);
                    drop(tool_result_guard);
                    drop(user_interaction_guard);
                    drop(permission_guard);
                    if let Some(payload) = msg {
                        self.handle_llm_response(payload).await;
                    } else {
                        tracing::info!("FromLLM channel closed");
                        break;
                    }
                }
                msg = input_guard.recv() => {
                    drop(from_llm_guard);
                    drop(input_guard);
                    drop(batch_result_guard);
                    drop(tool_result_guard);
                    drop(user_interaction_guard);
                    drop(permission_guard);
                    if let Some(payload) = msg {
                        self.handle_input(payload).await;
                    } else {
                        tracing::info!("Input channel closed");
                        break;
                    }
                }
                batch_result = batch_result_guard.recv() => {
                    drop(from_llm_guard);
                    drop(input_guard);
                    drop(batch_result_guard);
                    drop(tool_result_guard);
                    drop(user_interaction_guard);
                    drop(permission_guard);
                    if let Some(result) = batch_result {
                        self.handle_tool_batch_result(result).await;
                    }
                }
                tool_result = tool_result_guard.recv() => {
                    drop(from_llm_guard);
                    drop(input_guard);
                    drop(batch_result_guard);
                    drop(tool_result_guard);
                    drop(user_interaction_guard);
                    drop(permission_guard);
                    if let Some(result) = tool_result {
                        // Emit individual tool result for UI feedback
                        if let Some(ref func) = self.event_func {
                            func(ControllerEvent::ToolResult {
                                session_id: result.session_id,
                                tool_use_id: result.tool_use_id,
                                tool_name: result.tool_name,
                                display_name: result.display_name,
                                status: result.status,
                                content: result.content,
                                error: result.error,
                                turn_id: result.turn_id,
                            });
                        }
                    }
                }
                user_interaction_event = user_interaction_guard.recv() => {
                    drop(from_llm_guard);
                    drop(input_guard);
                    drop(batch_result_guard);
                    drop(tool_result_guard);
                    drop(user_interaction_guard);
                    drop(permission_guard);
                    if let Some(event) = user_interaction_event {
                        // Forward user interaction events to the event callback
                        if let Some(ref func) = self.event_func {
                            func(event);
                        }
                    }
                }
                permission_event = permission_guard.recv() => {
                    drop(from_llm_guard);
                    drop(input_guard);
                    drop(batch_result_guard);
                    drop(tool_result_guard);
                    drop(user_interaction_guard);
                    drop(permission_guard);
                    if let Some(event) = permission_event {
                        // Forward permission events to the event callback
                        if let Some(ref func) = self.event_func {
                            func(event);
                        }
                    }
                }
            }
        }

        tracing::info!("Controller stopped");
    }

    /// Handles a response from an LLM session
    async fn handle_llm_response(&self, payload: FromLLMPayload) {
        // Track token usage for TokenUpdate events
        if payload.response_type == LLMResponseType::TokenUpdate {
            if let Some(session) = self.session_mgr.get_session_by_id(payload.session_id).await {
                self.token_usage
                    .increment(
                        payload.session_id,
                        session.model(),
                        payload.input_tokens,
                        payload.output_tokens,
                    )
                    .await;
            }
        }

        let event = match payload.response_type {
            LLMResponseType::StreamStart => Some(ControllerEvent::StreamStart {
                session_id: payload.session_id,
                message_id: payload.message_id,
                model: payload.model,
                turn_id: payload.turn_id,
            }),
            LLMResponseType::TextChunk => Some(ControllerEvent::TextChunk {
                session_id: payload.session_id,
                text: payload.text,
                turn_id: payload.turn_id,
            }),
            LLMResponseType::ToolUseStart => {
                if let Some(tool) = payload.tool_use {
                    Some(ControllerEvent::ToolUseStart {
                        session_id: payload.session_id,
                        tool_id: tool.id,
                        tool_name: tool.name,
                        turn_id: payload.turn_id,
                    })
                } else {
                    None
                }
            }
            LLMResponseType::ToolInputDelta => {
                // Tool input deltas are internal - don't emit as event
                // The session accumulates them and emits complete ToolUse
                None
            }
            LLMResponseType::ToolUse => {
                if let Some(ref tool) = payload.tool_use {
                    // Execute the tool
                    let input: HashMap<String, serde_json::Value> = tool
                        .input
                        .as_object()
                        .map(|obj| {
                            obj.iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect()
                        })
                        .unwrap_or_default();

                    // Look up display name and title from tool registry (before moving input)
                    let (display_name, display_title) =
                        if let Some(t) = self.tool_registry().get(&tool.name).await {
                            let config = t.display_config();
                            (Some(config.display_name), Some((config.display_title)(&input)))
                        } else {
                            (None, None)
                        };

                    let request = ToolRequest {
                        tool_use_id: tool.id.clone(),
                        tool_name: tool.name.clone(),
                        input,
                    };

                    self.tool_executor
                        .execute(
                            payload.session_id,
                            payload.turn_id.clone(),
                            request,
                            self.cancel_token.clone(),
                        )
                        .await;

                    Some(ControllerEvent::ToolUse {
                        session_id: payload.session_id,
                        tool: payload.tool_use.unwrap(),
                        display_name,
                        display_title,
                        turn_id: payload.turn_id,
                    })
                } else {
                    None
                }
            }
            LLMResponseType::ToolBatch => {
                // Validate batch is not empty
                if payload.tool_uses.is_empty() {
                    tracing::error!(
                        session_id = payload.session_id,
                        "Received tool batch response with empty tool_uses"
                    );
                    return;
                }

                tracing::debug!(
                    session_id = payload.session_id,
                    tool_count = payload.tool_uses.len(),
                    "LLM requested tool batch execution"
                );

                // Build tool requests from payload
                let mut requests = Vec::with_capacity(payload.tool_uses.len());
                for tool_info in &payload.tool_uses {
                    let input: HashMap<String, serde_json::Value> = tool_info
                        .input
                        .as_object()
                        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default();

                    requests.push(ToolRequest {
                        tool_use_id: tool_info.id.clone(),
                        tool_name: tool_info.name.clone(),
                        input: input.clone(),
                    });

                    // Look up display name and title from tool registry
                    let (display_name, display_title) =
                        if let Some(tool) = self.tool_registry().get(&tool_info.name).await {
                            let config = tool.display_config();
                            (Some(config.display_name), Some((config.display_title)(&input)))
                        } else {
                            (None, None)
                        };

                    // Emit ToolUse event for UI
                    if let Some(ref func) = self.event_func {
                        func(ControllerEvent::ToolUse {
                            session_id: payload.session_id,
                            tool: tool_info.clone(),
                            display_name,
                            display_title,
                            turn_id: payload.turn_id.clone(),
                        });
                    }
                }

                // Execute batch - tools run concurrently, results sent when all complete
                self.tool_executor
                    .execute_batch(
                        payload.session_id,
                        payload.turn_id.clone(),
                        requests,
                        self.cancel_token.clone(),
                    )
                    .await;

                None
            }
            LLMResponseType::Complete => Some(ControllerEvent::Complete {
                session_id: payload.session_id,
                stop_reason: payload.stop_reason,
                turn_id: payload.turn_id,
            }),
            LLMResponseType::Error => Some(ControllerEvent::Error {
                session_id: payload.session_id,
                error: payload.error.unwrap_or_else(|| "Unknown error".to_string()),
                turn_id: payload.turn_id,
            }),
            LLMResponseType::TokenUpdate => {
                // Get context_limit from session
                let context_limit = if let Some(session) =
                    self.session_mgr.get_session_by_id(payload.session_id).await
                {
                    session.context_limit()
                } else {
                    0
                };
                Some(ControllerEvent::TokenUpdate {
                    session_id: payload.session_id,
                    input_tokens: payload.input_tokens,
                    output_tokens: payload.output_tokens,
                    context_limit,
                })
            }
        };

        // Emit event if we have one and a callback is registered
        if let (Some(event), Some(func)) = (event, &self.event_func) {
            func(event);
        }
    }

    /// Handles user input or control commands
    async fn handle_input(&self, payload: ControllerInputPayload) {
        match payload.input_type {
            InputType::Data => {
                self.handle_data_input(payload).await;
            }
            InputType::Control => {
                self.handle_control_input(payload).await;
            }
        }
    }

    /// Handles data input (user messages) by forwarding to the appropriate session
    async fn handle_data_input(&self, payload: ControllerInputPayload) {
        let session_id = payload.session_id;

        // Get the session
        let Some(session) = self.session_mgr.get_session_by_id(session_id).await else {
            tracing::error!(session_id, "Session not found for data input");
            self.emit_error(session_id, "Session not found".to_string(), payload.turn_id);
            return;
        };

        // Create the ToLLMPayload
        let llm_payload = ToLLMPayload {
            request_type: LLMRequestType::UserMessage,
            content: payload.content,
            tool_results: Vec::new(),
            options: None,
            turn_id: payload.turn_id,
            compact_summaries: HashMap::new(),
        };

        // Send to the session
        let sent = session.send(llm_payload).await;
        if !sent {
            tracing::error!(session_id, "Failed to send message to session");
            self.emit_error(
                session_id,
                "Failed to send message to session".to_string(),
                None,
            );
        }
    }

    /// Handles control commands (interrupt, shutdown)
    async fn handle_control_input(&self, payload: ControllerInputPayload) {
        let session_id = payload.session_id;

        let Some(cmd) = payload.control_cmd else {
            tracing::warn!(session_id, "Control input without command");
            return;
        };

        match cmd {
            ControlCmd::Interrupt => {
                // Interrupt the specific session
                if let Some(session) = self.session_mgr.get_session_by_id(session_id).await {
                    session.interrupt().await;
                    tracing::info!(session_id, "Session interrupted");
                } else {
                    tracing::warn!(session_id, "Cannot interrupt: session not found");
                }
            }
            ControlCmd::Shutdown => {
                // Shutdown the entire controller
                tracing::info!("Shutdown command received");
                self.shutdown().await;
            }
            ControlCmd::Clear => {
                // Clear conversation history for the session
                if let Some(session) = self.session_mgr.get_session_by_id(session_id).await {
                    session.clear_conversation().await;
                    tracing::info!(session_id, "Session conversation cleared");
                    self.emit_command_complete(session_id, cmd, true, None);
                } else {
                    tracing::warn!(session_id, "Cannot clear: session not found");
                    self.emit_command_complete(
                        session_id,
                        cmd,
                        false,
                        Some("Session not found".to_string()),
                    );
                }
            }
            ControlCmd::Compact => {
                // Trigger compaction for the session
                if let Some(session) = self.session_mgr.get_session_by_id(session_id).await {
                    let result = session.force_compact().await;

                    if let Some(error) = result.error {
                        // Compaction failed or no compactor configured
                        tracing::warn!(session_id, error = %error, "Session compaction failed");
                        self.emit_command_complete(session_id, cmd, false, Some(error));
                    } else if !result.compacted {
                        // Nothing to compact
                        tracing::info!(session_id, "Nothing to compact");
                        self.emit_command_complete(
                            session_id,
                            cmd,
                            true,
                            Some("Nothing to compact - not enough turns in conversation".to_string()),
                        );
                    } else {
                        // Compaction succeeded
                        let message = format!(
                            "Conversation compacted\n  Turns summarized: {}\n  Turns kept: {}\n  Messages: {} -> {}{}",
                            result.turns_compacted,
                            result.turns_kept,
                            result.messages_before,
                            result.messages_after,
                            if result.summary_length > 0 {
                                format!("\n  Summary length: {} chars", result.summary_length)
                            } else {
                                String::new()
                            }
                        );
                        tracing::info!(
                            session_id,
                            turns_compacted = result.turns_compacted,
                            messages_before = result.messages_before,
                            messages_after = result.messages_after,
                            "Session compaction completed"
                        );
                        self.emit_command_complete(session_id, cmd, true, Some(message));
                    }
                } else {
                    tracing::warn!(session_id, "Cannot compact: session not found");
                    self.emit_command_complete(
                        session_id,
                        cmd,
                        false,
                        Some("Session not found".to_string()),
                    );
                }
            }
        }
    }

    /// Emits an error event
    fn emit_error(&self, session_id: i64, error: String, turn_id: Option<TurnId>) {
        if let Some(ref func) = self.event_func {
            func(ControllerEvent::Error {
                session_id,
                error,
                turn_id,
            });
        }
    }

    /// Emits a command complete event
    fn emit_command_complete(
        &self,
        session_id: i64,
        command: ControlCmd,
        success: bool,
        message: Option<String>,
    ) {
        if let Some(ref func) = self.event_func {
            func(ControllerEvent::CommandComplete {
                session_id,
                command,
                success,
                message,
            });
        }
    }

    /// Handles a batch of tool execution results by sending them back to the session.
    async fn handle_tool_batch_result(&self, batch_result: ToolBatchResult) {
        use crate::controller::types::ToolResultInfo;

        let session_id = batch_result.session_id;

        let Some(session) = self.session_mgr.get_session_by_id(session_id).await else {
            tracing::error!(session_id, "Session not found for tool result");
            return;
        };

        // Convert batch results to ToolResultInfo and extract compact summaries
        let mut compact_summaries = HashMap::new();
        let tool_results: Vec<ToolResultInfo> = batch_result
            .results
            .iter()
            .map(|result| {
                let (content, is_error) = if let Some(ref error) = result.error {
                    (error.clone(), true)
                } else {
                    (result.content.clone(), false)
                };

                // Extract compact summary for compaction
                if let Some(ref summary) = result.compact_summary {
                    compact_summaries.insert(result.tool_use_id.clone(), summary.clone());
                    tracing::debug!(
                        tool_use_id = %result.tool_use_id,
                        summary_len = summary.len(),
                        "Extracted compact summary for tool result"
                    );
                }

                ToolResultInfo {
                    tool_use_id: result.tool_use_id.clone(),
                    content,
                    is_error,
                }
            })
            .collect();

        tracing::info!(
            session_id,
            tool_count = tool_results.len(),
            compact_summary_count = compact_summaries.len(),
            "Sending tool results to session with compact summaries"
        );

        // Create tool result payload
        let llm_payload = ToLLMPayload {
            request_type: LLMRequestType::ToolResult,
            content: String::new(),
            tool_results,
            options: None,
            turn_id: batch_result.turn_id,
            compact_summaries,
        };

        // Send to the session to continue the conversation
        let sent = session.send(llm_payload).await;
        if !sent {
            tracing::error!(session_id, "Failed to send tool result to session");
        } else {
            tracing::debug!(
                session_id,
                batch_id = batch_result.batch_id,
                "Sent tool results to session"
            );
        }
    }

    /// Gracefully shuts down the controller and all sessions.
    /// This is idempotent - calling it multiple times has no effect.
    pub async fn shutdown(&self) {
        if self
            .shutdown
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return; // Already shutdown
        }

        tracing::info!("Controller shutting down");

        // Shutdown all sessions
        self.session_mgr.shutdown().await;

        // Cancel the event loop
        self.cancel_token.cancel();

        tracing::info!("Controller shutdown complete");
    }

    /// Returns true if the controller has been shutdown
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// Returns true if the controller has been started
    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    // ---- Session Management ----

    /// Creates a new LLM session.
    ///
    /// # Arguments
    /// * `config` - Session configuration (includes model, API key, etc.)
    ///
    /// # Returns
    /// The session ID of the newly created session
    ///
    /// # Errors
    /// Returns an error if the session fails to initialize (e.g., TLS setup failure)
    pub async fn create_session(&self, config: LLMSessionConfig) -> Result<i64, LlmError> {
        let session_id = self
            .session_mgr
            .create_session(config, self.from_llm_tx.clone())
            .await?;

        tracing::info!(session_id, "Session created via controller");
        Ok(session_id)
    }

    /// Retrieves a session by its ID.
    ///
    /// # Returns
    /// The session if found, None otherwise
    pub async fn get_session(&self, session_id: i64) -> Option<Arc<LLMSession>> {
        self.session_mgr.get_session_by_id(session_id).await
    }

    /// Returns the number of active sessions
    pub async fn session_count(&self) -> usize {
        self.session_mgr.session_count().await
    }

    // ---- Input Handling ----

    /// Sends input to the controller for processing.
    ///
    /// # Arguments
    /// * `input` - The input payload to send
    ///
    /// # Returns
    /// Ok(()) if the input was sent successfully, Err with message otherwise
    pub async fn send_input(&self, input: ControllerInputPayload) -> Result<(), String> {
        if self.is_shutdown() {
            return Err("Controller is shutdown".to_string());
        }

        match tokio::time::timeout(SEND_INPUT_TIMEOUT, self.input_tx.send(input)).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err("Input channel closed".to_string()),
            Err(_) => Err("Timeout sending input".to_string()),
        }
    }

    // ---- Token Usage ----

    /// Get token usage for a specific session.
    pub async fn get_session_token_usage(
        &self,
        session_id: i64,
    ) -> Option<crate::controller::usage::TokenMeter> {
        self.token_usage.get_session_usage(session_id).await
    }

    /// Get token usage for a specific model.
    pub async fn get_model_token_usage(&self, model: &str) -> Option<crate::controller::usage::TokenMeter> {
        self.token_usage.get_model_usage(model).await
    }

    /// Get total token usage across all sessions.
    pub async fn get_total_token_usage(&self) -> crate::controller::usage::TokenMeter {
        self.token_usage.get_total_usage().await
    }

    /// Get a reference to the token usage tracker for advanced queries.
    pub fn token_usage(&self) -> &TokenUsageTracker {
        &self.token_usage
    }

    // ---- Tool Registry ----

    /// Returns a reference to the tool registry.
    pub fn tool_registry(&self) -> &Arc<ToolRegistry> {
        &self.tool_registry
    }

    // ---- User Interaction ----

    /// Returns a reference to the user interaction registry.
    pub fn user_interaction_registry(&self) -> &Arc<UserInteractionRegistry> {
        &self.user_interaction_registry
    }

    /// Respond to a pending user interaction.
    ///
    /// This is called by the UI when the user has answered questions.
    ///
    /// # Arguments
    /// * `tool_use_id` - ID of the tool use to respond to.
    /// * `response` - The user's answers.
    pub async fn respond_to_interaction(
        &self,
        tool_use_id: &str,
        response: AskUserQuestionsResponse,
    ) -> Result<(), UserInteractionError> {
        self.user_interaction_registry
            .respond(tool_use_id, response)
            .await
    }

    /// Get all pending interactions for a session.
    ///
    /// This is called by the UI when switching sessions to display
    /// any pending questions for that session.
    pub async fn pending_interactions_for_session(
        &self,
        session_id: i64,
    ) -> Vec<PendingQuestionInfo> {
        self.user_interaction_registry
            .pending_for_session(session_id)
            .await
    }

    /// Check if a session has pending user interactions.
    pub async fn has_pending_interactions(&self, session_id: i64) -> bool {
        self.user_interaction_registry.has_pending(session_id).await
    }

    // ---- Permission Management ----

    /// Returns a reference to the permission registry.
    pub fn permission_registry(&self) -> &Arc<PermissionRegistry> {
        &self.permission_registry
    }

    /// Respond to a pending permission request.
    ///
    /// This is called by the UI when the user has granted or denied permission.
    ///
    /// # Arguments
    /// * `tool_use_id` - ID of the tool use to respond to.
    /// * `response` - The user's response (grant/deny with scope).
    pub async fn respond_to_permission(
        &self,
        tool_use_id: &str,
        response: PermissionResponse,
    ) -> Result<(), PermissionError> {
        self.permission_registry
            .respond(tool_use_id, response)
            .await
    }

    /// Get all pending permission requests for a session.
    ///
    /// This is called by the UI when switching sessions to display
    /// any pending permission requests for that session.
    pub async fn pending_permissions_for_session(
        &self,
        session_id: i64,
    ) -> Vec<PendingPermissionInfo> {
        self.permission_registry
            .pending_for_session(session_id)
            .await
    }

    /// Check if a session has pending permission requests.
    pub async fn has_pending_permissions(&self, session_id: i64) -> bool {
        self.permission_registry.has_pending(session_id).await
    }

    /// Cancel a pending permission request.
    ///
    /// This is called by the UI when the user closes the permission dialog
    /// without responding.
    pub async fn cancel_permission(&self, tool_use_id: &str) -> Result<(), PermissionError> {
        self.permission_registry.cancel(tool_use_id).await
    }
}
