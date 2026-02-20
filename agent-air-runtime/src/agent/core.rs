// AgentAir - Core runtime infrastructure for LLM-powered agents
//
// This module provides the runtime engine without any TUI dependencies.
// For TUI functionality, use agent-air-tui which extends this with run().

use std::io;
use std::sync::Arc;

use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::controller::{
    ControllerEvent, ControllerInputPayload, Executable, LLMController, LLMSessionConfig, LLMTool,
    ListSkillsTool, PermissionRegistry, ToolRegistry, UserInteractionRegistry,
};
use crate::skills::{SkillDiscovery, SkillDiscoveryError, SkillRegistry, SkillReloadResult};

use super::config::{AgentConfig, LLMRegistry, load_config};
use super::error::AgentError;
use super::logger::Logger;
use super::messages::UiMessage;
use super::messages::channels::DEFAULT_CHANNEL_SIZE;
use super::router::InputRouter;

/// Sender for messages from frontend to controller
pub type ToControllerTx = mpsc::Sender<ControllerInputPayload>;
/// Receiver for messages from frontend to controller
pub type ToControllerRx = mpsc::Receiver<ControllerInputPayload>;
/// Sender for messages from controller to frontend
pub type FromControllerTx = mpsc::Sender<UiMessage>;
/// Receiver for messages from controller to frontend
pub type FromControllerRx = mpsc::Receiver<UiMessage>;

/// AgentAir - Core runtime infrastructure for LLM-powered agents.
///
/// AgentAir provides all the infrastructure needed for an LLM-powered agent:
/// - Logging with tracing
/// - LLM configuration loading
/// - Tokio async runtime
/// - LLMController for session management
/// - Communication channels
/// - User interaction and permission registries
///
/// This is the runtime-only version. For TUI support, use the `agent-air` crate
/// with the `tui` feature enabled, which provides the `run()` method.
///
/// # Basic Usage (Headless)
///
/// ```ignore
/// use agent_air_runtime::agent::{AgentConfig, AgentAir};
///
/// struct MyConfig;
/// impl AgentConfig for MyConfig {
///     fn state_dir(&self) -> &str { "~/.myagent" }
///     fn default_system_prompt(&self) -> &str { "You are helpful." }
///     fn log_prefix(&self) -> &str { "myagent" }
///     fn name(&self) -> &str { "MyAgent" }
/// }
///
/// fn main() -> std::io::Result<()> {
///     let mut core = AgentAir::new(&MyConfig)?;
///     core.start_background_tasks();
///
///     // Get channels for custom frontend integration
///     let tx = core.to_controller_tx();
///     let rx = core.take_from_controller_rx();
///
///     // Create a session and interact programmatically
///     let (session_id, model, _) = core.create_initial_session()?;
///     // ... send messages and receive responses via channels
///
///     core.shutdown();
///     Ok(())
/// }
/// ```
pub struct AgentAir {
    /// Logger instance - never directly accessed but must be kept alive for RAII.
    /// Dropping this field would stop logging, so it's held for the lifetime of AgentAir.
    #[allow(dead_code)]
    logger: Logger,

    /// Agent name for display
    name: String,

    /// Resolved state directory (absolute path)
    state_dir: std::path::PathBuf,

    /// Agent version for display
    version: String,

    /// Tokio runtime for async operations
    runtime: Runtime,

    /// The LLM controller
    controller: Arc<LLMController>,

    /// LLM provider registry (loaded from config)
    llm_registry: Option<LLMRegistry>,

    /// Sender for messages from frontend to controller
    to_controller_tx: ToControllerTx,

    /// Receiver for messages from frontend to controller (consumed by InputRouter)
    to_controller_rx: Option<ToControllerRx>,

    /// Sender for messages from controller to frontend (held by event handler)
    from_controller_tx: FromControllerTx,

    /// Receiver for messages from controller to frontend
    from_controller_rx: Option<FromControllerRx>,

    /// Cancellation token for graceful shutdown
    cancel_token: CancellationToken,

    /// User interaction registry for AskUserQuestions tool
    user_interaction_registry: Arc<UserInteractionRegistry>,

    /// Permission registry for AskForPermissions tool
    permission_registry: Arc<PermissionRegistry>,

    /// Tool definitions to register on sessions
    tool_definitions: Vec<LLMTool>,

    /// Error message shown when user submits but no session exists
    error_no_session: Option<String>,

    /// Skill registry for Agent Skills support
    skill_registry: Arc<SkillRegistry>,

    /// Skill discovery paths
    skill_discovery: SkillDiscovery,

    /// Optional embedded database
    #[cfg(feature = "db")]
    database: Option<crate::db::AgentDatabase>,
}

impl AgentAir {
    /// Create a new AgentAir with the given configuration.
    ///
    /// This initializes:
    /// - Logging infrastructure
    /// - LLM configuration from config file or environment
    /// - Tokio runtime
    /// - Communication channels
    /// - LLMController
    /// - User interaction and permission registries
    pub fn new<C: AgentConfig>(config: &C) -> io::Result<Self> {
        let logger = Logger::new(config.log_prefix())?;
        tracing::info!("{} agent initialized", config.name());

        // Load LLM configuration
        let llm_registry = load_config(config);
        if llm_registry.is_empty() {
            tracing::warn!(
                "No LLM providers configured. Set ANTHROPIC_API_KEY or create {}/config.yaml",
                config.state_dir()
            );
        } else {
            tracing::info!(
                "Loaded {} LLM provider(s): {:?}",
                llm_registry.providers().len(),
                llm_registry.providers()
            );
        }

        // Create tokio runtime for async operations
        let runtime = Runtime::new()
            .map_err(|e| io::Error::other(format!("Failed to create runtime: {}", e)))?;

        // Get channel buffer size from config (or use default)
        let channel_size = config.channel_buffer_size().unwrap_or(DEFAULT_CHANNEL_SIZE);
        tracing::debug!("Using channel buffer size: {}", channel_size);

        // Create communication channels
        let (to_controller_tx, to_controller_rx) =
            mpsc::channel::<ControllerInputPayload>(channel_size);
        let (from_controller_tx, from_controller_rx) = mpsc::channel::<UiMessage>(channel_size);

        // Create channel for user interaction events
        let (interaction_event_tx, mut interaction_event_rx) =
            mpsc::channel::<ControllerEvent>(channel_size);

        // Create the user interaction registry
        let user_interaction_registry =
            Arc::new(UserInteractionRegistry::new(interaction_event_tx));

        // Spawn a task to forward user interaction events to the UI channel
        // Uses blocking send for backpressure
        let ui_tx_for_interactions = from_controller_tx.clone();
        runtime.spawn(async move {
            while let Some(event) = interaction_event_rx.recv().await {
                let msg = convert_controller_event_to_ui_message(event);
                if let Err(e) = ui_tx_for_interactions.send(msg).await {
                    tracing::warn!("Failed to send user interaction event to UI: {}", e);
                }
            }
        });

        // Create channel for permission events
        let (permission_event_tx, mut permission_event_rx) =
            mpsc::channel::<ControllerEvent>(channel_size);

        // Create the permission registry
        let permission_registry = Arc::new(PermissionRegistry::new(permission_event_tx));

        // Spawn a task to forward permission events to the UI channel
        // Uses blocking send for backpressure
        let ui_tx_for_permissions = from_controller_tx.clone();
        runtime.spawn(async move {
            while let Some(event) = permission_event_rx.recv().await {
                let msg = convert_controller_event_to_ui_message(event);
                if let Err(e) = ui_tx_for_permissions.send(msg).await {
                    tracing::warn!("Failed to send permission event to UI: {}", e);
                }
            }
        });

        // Create the controller with UI channel for direct event forwarding
        // The controller will use backpressure: when UI channel is full, it stops
        // reading from LLM, which backs up the from_llm channel, which blocks the
        // session, which slows down network consumption.
        let controller = Arc::new(LLMController::new(
            permission_registry.clone(),
            Some(from_controller_tx.clone()),
            Some(channel_size),
        ));
        let cancel_token = CancellationToken::new();

        // Resolve state directory to an absolute path
        let raw_state_dir = config.state_dir();
        let state_dir = if let Some(rest) = raw_state_dir.strip_prefix("~/") {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from(raw_state_dir))
                .join(rest)
        } else {
            std::path::PathBuf::from(raw_state_dir)
        };

        Ok(Self {
            logger,
            name: config.name().to_string(),
            state_dir,
            version: "0.1.0".to_string(),
            runtime,
            controller,
            llm_registry: Some(llm_registry),
            to_controller_tx,
            to_controller_rx: Some(to_controller_rx),
            from_controller_tx,
            from_controller_rx: Some(from_controller_rx),
            cancel_token,
            user_interaction_registry,
            permission_registry,
            tool_definitions: Vec::new(),
            error_no_session: None,
            skill_registry: Arc::new(SkillRegistry::new()),
            skill_discovery: SkillDiscovery::new(),
            #[cfg(feature = "db")]
            database: None,
        })
    }

    /// Create a new AgentAir with simple configuration parameters.
    ///
    /// This is a convenience constructor for quick agent setup without
    /// defining a custom config struct.
    ///
    /// # Arguments
    /// * `name` - Agent name for display (e.g., "my-agent")
    /// * `state_dir` - State directory (e.g., "~/.my-agent")
    /// * `system_prompt` - Default system prompt for the agent
    ///
    /// # Example
    ///
    /// ```ignore
    /// use agent_air::agent::AgentAir;
    /// use agent_air::tui::AgentAirExt;
    ///
    /// AgentAir::with_config("my-agent", "~/.my-agent", "You are helpful.")?
    ///     .into_tui()
    ///     .run()
    /// ```
    pub fn with_config(
        name: impl Into<String>,
        state_dir: impl Into<String>,
        system_prompt: impl Into<String>,
    ) -> io::Result<Self> {
        let config = super::config::SimpleConfig::new(name, state_dir, system_prompt);
        Self::new(&config)
    }

    /// Set the error message shown when user submits but no session exists.
    ///
    /// This overrides the default message "No active session. Use /new-session to create one."
    ///
    /// # Example
    ///
    /// ```ignore
    /// agent.set_error_no_session("No configuration found in ~/.myagent/config.yaml");
    /// ```
    pub fn set_error_no_session(&mut self, message: impl Into<String>) -> &mut Self {
        self.error_no_session = Some(message.into());
        self
    }

    /// Get the error message for no session, if set.
    pub fn error_no_session(&self) -> Option<&str> {
        self.error_no_session.as_deref()
    }

    /// Set the agent version for display.
    pub fn set_version(&mut self, version: impl Into<String>) {
        self.version = version.into();
    }

    /// Get the agent version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Load environment context into the system prompt.
    ///
    /// This adds information about the current execution environment to
    /// all LLM session prompts:
    /// - Current working directory
    /// - Platform (darwin, linux, windows)
    /// - OS version
    /// - Today's date
    ///
    /// The context is wrapped in `<env>` tags and appended to the system prompt.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut core = AgentAir::new(&config)?;
    /// core.load_environment_context();
    /// ```
    pub fn load_environment_context(&mut self) -> &mut Self {
        if let Some(registry) = self.llm_registry.take() {
            self.llm_registry = Some(registry.with_environment_context());
            tracing::info!("Environment context loaded into system prompt");
        }
        self
    }

    /// Register tools with the agent.
    ///
    /// The callback receives references to the tool registry and interaction registries,
    /// and should return the tool definitions to register.
    ///
    /// # Example
    ///
    /// ```ignore
    /// core.register_tools(|registry, user_reg, perm_reg| {
    ///     tools::register_all_tools(registry, user_reg, perm_reg)
    /// })?;
    /// ```
    pub fn register_tools<F>(&mut self, f: F) -> Result<(), AgentError>
    where
        F: FnOnce(
            &Arc<ToolRegistry>,
            &Arc<UserInteractionRegistry>,
            &Arc<PermissionRegistry>,
        ) -> Result<Vec<LLMTool>, String>,
    {
        let tool_defs = f(
            self.controller.tool_registry(),
            &self.user_interaction_registry,
            &self.permission_registry,
        )
        .map_err(AgentError::ToolRegistration)?;
        self.tool_definitions = tool_defs;
        Ok(())
    }

    /// Register tools with the agent using an async function.
    ///
    /// Similar to `register_tools`, but accepts an async closure. The closure
    /// is executed using the agent's tokio runtime via `block_on`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// core.register_tools_async(|registry, user_reg, perm_reg| async move {
    ///     tools::register_all_tools(&registry, user_reg, perm_reg).await
    /// })?;
    /// ```
    pub fn register_tools_async<F, Fut>(&mut self, f: F) -> Result<(), AgentError>
    where
        F: FnOnce(Arc<ToolRegistry>, Arc<UserInteractionRegistry>, Arc<PermissionRegistry>) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<LLMTool>, String>>,
    {
        let tool_defs = self
            .runtime
            .block_on(f(
                self.controller.tool_registry().clone(),
                self.user_interaction_registry.clone(),
                self.permission_registry.clone(),
            ))
            .map_err(AgentError::ToolRegistration)?;
        self.tool_definitions = tool_defs;
        Ok(())
    }

    /// Start the controller and input router as background tasks.
    ///
    /// This must be called before sending messages or creating sessions.
    /// After calling this, the controller is running and ready to accept input.
    pub fn start_background_tasks(&mut self) {
        tracing::info!("{} starting background tasks", self.name);

        // Start the controller event loop in a background task
        let controller = self.controller.clone();
        self.runtime.spawn(async move {
            controller.start().await;
        });
        tracing::info!("Controller started");

        // Start the input router in a background task
        if let Some(to_controller_rx) = self.to_controller_rx.take() {
            let router = InputRouter::new(
                self.controller.clone(),
                to_controller_rx,
                self.cancel_token.clone(),
            );
            self.runtime.spawn(async move {
                router.run().await;
            });
            tracing::info!("InputRouter started");
        }
    }

    /// Internal helper to create a session and configure tools.
    async fn create_session_internal(
        controller: &Arc<LLMController>,
        mut config: LLMSessionConfig,
        tools: &[LLMTool],
        skill_registry: &Arc<SkillRegistry>,
    ) -> Result<i64, crate::client::error::LlmError> {
        // Inject skills XML into system prompt
        let skills_xml = skill_registry.to_prompt_xml();
        if !skills_xml.is_empty() {
            config.system_prompt = Some(match config.system_prompt {
                Some(prompt) => format!("{}\n\n{}", prompt, skills_xml),
                None => skills_xml,
            });
        }

        let id = controller.create_session(config).await?;

        // Set tools on the session after creation
        if !tools.is_empty()
            && let Some(session) = controller.get_session(id).await
        {
            session.set_tools(tools.to_vec()).await;
        }

        Ok(id)
    }

    /// Create an initial session using the default LLM provider.
    ///
    /// Returns the session ID, model name, and context limit.
    pub fn create_initial_session(&mut self) -> Result<(i64, String, i32), AgentError> {
        let registry = self
            .llm_registry
            .as_ref()
            .ok_or_else(|| AgentError::NoConfiguration("No LLM registry available".to_string()))?;

        let config = registry.get_default().ok_or_else(|| {
            AgentError::NoConfiguration("No default LLM provider configured".to_string())
        })?;

        let model = config.model.clone();
        let context_limit = config.context_limit;

        let controller = self.controller.clone();
        let tool_definitions = self.tool_definitions.clone();
        let skill_registry = self.skill_registry.clone();

        let session_id = self.runtime.block_on(Self::create_session_internal(
            &controller,
            config.clone(),
            &tool_definitions,
            &skill_registry,
        ))?;

        tracing::info!(
            session_id = session_id,
            model = %model,
            "Created initial session"
        );

        Ok((session_id, model, context_limit))
    }

    /// Create a session with the given configuration.
    ///
    /// Returns the session ID or an error.
    pub fn create_session(&self, config: LLMSessionConfig) -> Result<i64, AgentError> {
        let controller = self.controller.clone();
        let tool_definitions = self.tool_definitions.clone();
        let skill_registry = self.skill_registry.clone();

        self.runtime
            .block_on(Self::create_session_internal(
                &controller,
                config,
                &tool_definitions,
                &skill_registry,
            ))
            .map_err(AgentError::from)
    }

    /// Signal shutdown to all background tasks and the controller.
    pub fn shutdown(&self) {
        tracing::info!("{} shutting down", self.name);
        self.cancel_token.cancel();

        let controller = self.controller.clone();
        self.runtime.block_on(async move {
            controller.shutdown().await;
        });

        tracing::info!("{} shutdown complete", self.name);
    }

    // ---- Custom Frontend Support ----

    /// Run the agent with a custom frontend.
    ///
    /// This is the primary entry point for custom frontends. It:
    /// 1. Starts background tasks (controller, input router)
    /// 2. Wires the event sink to receive engine events
    /// 3. Wires the input source to provide user input
    /// 4. Applies the permission policy
    /// 5. Runs until the input source closes
    ///
    /// # Arguments
    ///
    /// * `event_sink` - Receives events from the engine
    /// * `input_source` - Provides input to the engine
    /// * `permission_policy` - Handles permission requests
    ///
    /// # Example: Headless with Auto-Approve
    ///
    /// ```ignore
    /// use agent_air_runtime::agent::{
    ///     AgentAir, AutoApprovePolicy, StdoutEventSink, ChannelInputSource
    /// };
    ///
    /// let mut agent = AgentAir::with_config(
    ///     "my-agent",
    ///     "~/.config/my-agent/config.yaml",
    ///     "You are helpful."
    /// )?;
    ///
    /// // Create input channel
    /// let (input_tx, input_source) = ChannelInputSource::channel(100);
    ///
    /// // Run with custom frontend (blocks until input_tx is dropped)
    /// agent.run_with_frontend(
    ///     StdoutEventSink::new(),
    ///     input_source,
    ///     AutoApprovePolicy::new(),
    /// )?;
    /// ```
    pub fn run_with_frontend<E, I, P>(
        &mut self,
        event_sink: E,
        mut input_source: I,
        permission_policy: P,
    ) -> io::Result<()>
    where
        E: super::interface::EventSink,
        I: super::interface::InputSource,
        P: super::interface::PermissionPolicy,
    {
        use super::interface::PolicyDecision;
        use crate::permissions::{BatchPermissionResponse, PermissionPanelResponse};
        use std::sync::Arc;

        tracing::info!("{} starting with custom frontend", self.name);

        // Wrap sink in Arc for sharing with event forwarder
        let sink = Arc::new(event_sink);
        let policy = Arc::new(permission_policy);

        // Start background tasks (controller, but not the default input router)
        // We'll handle input ourselves
        let controller = self.controller.clone();
        self.runtime.spawn(async move {
            controller.start().await;
        });
        tracing::info!("Controller started");

        // Set up event forwarding from controller to custom sink
        // Take the from_controller_rx if available
        if let Some(mut from_controller_rx) = self.from_controller_rx.take() {
            let sink_clone = sink.clone();
            let policy_clone = policy.clone();
            let permission_registry = self.permission_registry.clone();
            let user_interaction_registry = self.user_interaction_registry.clone();

            self.runtime.spawn(async move {
                while let Some(event) = from_controller_rx.recv().await {
                    // Check if this is a permission request that should be handled by policy
                    match &event {
                        UiMessage::PermissionRequired {
                            tool_use_id,
                            request,
                            ..
                        } => {
                            match policy_clone.decide(request) {
                                PolicyDecision::AskUser => {
                                    // Fall through to forward to sink
                                }
                                decision => {
                                    let response = match decision {
                                        PolicyDecision::Allow => PermissionPanelResponse {
                                            granted: true,
                                            grant: None,
                                            message: None,
                                        },
                                        PolicyDecision::AllowWithGrant(grant) => {
                                            PermissionPanelResponse {
                                                granted: true,
                                                grant: Some(grant),
                                                message: None,
                                            }
                                        }
                                        PolicyDecision::Deny { reason } => {
                                            PermissionPanelResponse {
                                                granted: false,
                                                grant: None,
                                                message: reason,
                                            }
                                        }
                                        PolicyDecision::AskUser => unreachable!(),
                                    };
                                    if let Err(e) = permission_registry
                                        .respond_to_request(tool_use_id, response)
                                        .await
                                    {
                                        tracing::warn!(
                                            "Failed to respond to permission request: {}",
                                            e
                                        );
                                    }
                                    continue; // Don't forward to sink
                                }
                            }
                        }
                        UiMessage::BatchPermissionRequired { batch, .. } => {
                            // Check if policy handles all requests in the batch
                            let mut all_handled = true;
                            let mut approved_grants = Vec::new();
                            let mut denied_ids = Vec::new();

                            for request in &batch.requests {
                                match policy_clone.decide(request) {
                                    PolicyDecision::Allow => {
                                        // No grant to add, but approved
                                    }
                                    PolicyDecision::AllowWithGrant(grant) => {
                                        approved_grants.push(grant);
                                    }
                                    PolicyDecision::Deny { .. } => {
                                        denied_ids.push(request.id.clone());
                                    }
                                    PolicyDecision::AskUser => {
                                        all_handled = false;
                                        break;
                                    }
                                }
                            }

                            if all_handled {
                                // Respond to batch with policy decisions
                                let response = if denied_ids.is_empty() {
                                    BatchPermissionResponse::all_granted(
                                        &batch.batch_id,
                                        approved_grants,
                                    )
                                } else {
                                    BatchPermissionResponse::all_denied(&batch.batch_id, denied_ids)
                                };
                                if let Err(e) = permission_registry
                                    .respond_to_batch(&batch.batch_id, response)
                                    .await
                                {
                                    tracing::warn!(
                                        "Failed to respond to batch permission request: {}",
                                        e
                                    );
                                }
                                continue; // Don't forward to sink
                            }
                            // Fall through to forward to sink if any request needs user input
                        }
                        UiMessage::UserInteractionRequired { tool_use_id, .. } => {
                            if !policy_clone.supports_interaction() {
                                // Headless mode - auto-cancel the interaction
                                if let Err(e) = user_interaction_registry.cancel(tool_use_id).await
                                {
                                    tracing::warn!("Failed to cancel user interaction: {}", e);
                                }
                                tracing::debug!("Auto-cancelled user interaction in headless mode");
                                continue; // Don't forward to sink
                            }
                            // Fall through to forward to sink for interactive policies
                        }
                        _ => {}
                    }

                    // Forward event to sink
                    if let Err(e) = sink_clone.send(event) {
                        tracing::warn!("Failed to send event to sink: {}", e);
                    }
                }
            });
        }

        // Create initial session if configured
        match self.create_initial_session() {
            Ok((session_id, model, _)) => {
                tracing::info!(session_id, model = %model, "Created initial session");
            }
            Err(e) => {
                tracing::warn!(error = %e, "No initial session created");
            }
        }

        // Run input loop - forward input from source to controller
        let to_controller_tx = self.to_controller_tx.clone();
        self.runtime.block_on(async {
            while let Some(input) = input_source.recv().await {
                if let Err(e) = to_controller_tx.send(input).await {
                    tracing::error!(error = %e, "Failed to send input to controller");
                    break;
                }
            }
        });

        // Shutdown
        self.shutdown();
        tracing::info!("{} stopped", self.name);

        Ok(())
    }

    // ---- Accessors ----

    /// Returns a sender for sending messages to the controller.
    pub fn to_controller_tx(&self) -> ToControllerTx {
        self.to_controller_tx.clone()
    }

    /// Takes the receiver for messages from the controller (can only be called once).
    pub fn take_from_controller_rx(&mut self) -> Option<FromControllerRx> {
        self.from_controller_rx.take()
    }

    /// Returns a reference to the controller.
    pub fn controller(&self) -> &Arc<LLMController> {
        &self.controller
    }

    /// Returns a reference to the runtime.
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// Returns a handle to the runtime.
    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    /// Returns a reference to the user interaction registry.
    pub fn user_interaction_registry(&self) -> &Arc<UserInteractionRegistry> {
        &self.user_interaction_registry
    }

    /// Returns a reference to the permission registry.
    pub fn permission_registry(&self) -> &Arc<PermissionRegistry> {
        &self.permission_registry
    }

    /// Removes a session and cleans up all associated resources.
    ///
    /// This is the recommended way to remove a session as it orchestrates cleanup across:
    /// - The LLM session manager (terminates the session)
    /// - The permission registry (cancels pending permission requests)
    /// - The user interaction registry (cancels pending user questions)
    /// - The tool registry (cleans up per-session state in tools)
    ///
    /// # Arguments
    /// * `session_id` - The ID of the session to remove
    ///
    /// # Returns
    /// true if the session was found and removed, false if session didn't exist
    pub async fn remove_session(&self, session_id: i64) -> bool {
        // Remove from controller's session manager
        let removed = self.controller.remove_session(session_id).await;

        // Clean up pending permission requests for this session
        self.permission_registry.cancel_session(session_id).await;

        // Clean up pending user interactions for this session
        self.user_interaction_registry
            .cancel_session(session_id)
            .await;

        // Clean up per-session state in tools (e.g., bash working directories)
        self.controller
            .tool_registry()
            .cleanup_session(session_id)
            .await;

        if removed {
            tracing::info!(session_id, "Session removed with full cleanup");
        }

        removed
    }

    /// Returns a reference to the LLM registry.
    pub fn llm_registry(&self) -> Option<&LLMRegistry> {
        self.llm_registry.as_ref()
    }

    /// Takes the LLM registry (can only be called once).
    pub fn take_llm_registry(&mut self) -> Option<LLMRegistry> {
        self.llm_registry.take()
    }

    /// Returns the cancellation token.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Returns the agent name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the resolved state directory path.
    pub fn state_dir(&self) -> &std::path::Path {
        &self.state_dir
    }

    /// Returns a clone of the UI message sender.
    ///
    /// This can be used to send messages to the frontend's event loop.
    pub fn from_controller_tx(&self) -> FromControllerTx {
        self.from_controller_tx.clone()
    }

    /// Returns a reference to the tool definitions.
    pub fn tool_definitions(&self) -> &[LLMTool] {
        &self.tool_definitions
    }

    // ---- Database ----

    /// Initialize the embedded LMDB database.
    ///
    /// The database directory is created under `data_dir`. This is opt-in:
    /// call this method before accessing [`AgentAir::db`].
    #[cfg(feature = "db")]
    pub fn enable_database(
        &mut self,
        data_dir: &std::path::Path,
        config: &crate::db::DbConfig,
    ) -> Result<&mut Self, super::error::AgentError> {
        let db = crate::db::AgentDatabase::open(data_dir, config)?;
        self.database = Some(db);
        tracing::info!(path = %data_dir.display(), "Database enabled");
        Ok(self)
    }

    /// Initialize the embedded database, creating it if needed.
    ///
    /// This is idempotent: if the database is already enabled it returns
    /// immediately. Otherwise it opens (or creates) an LMDB environment at
    /// `{state_dir}/db/` with default [`DbConfig`](crate::db::DbConfig).
    #[cfg(feature = "db")]
    pub fn init_database(&mut self) -> Result<&mut Self, super::error::AgentError> {
        if self.database.is_some() {
            return Ok(self);
        }

        let data_dir = self.state_dir.join("db");
        self.enable_database(&data_dir, &crate::db::DbConfig::default())
    }

    /// Returns a reference to the database, if enabled.
    #[cfg(feature = "db")]
    pub fn db(&self) -> Option<&crate::db::AgentDatabase> {
        self.database.as_ref()
    }

    // ---- Skills ----

    /// Returns a reference to the skill registry.
    pub fn skill_registry(&self) -> &Arc<SkillRegistry> {
        &self.skill_registry
    }

    /// Register the ListSkillsTool, allowing the LLM to discover available skills.
    ///
    /// This registers the `list_skills` tool with the tool registry and adds its
    /// definition to the tool list. Call this after `register_tools()` if you want
    /// the LLM to be able to query available skills.
    ///
    /// Returns the LLM tool definition that was added.
    pub fn register_list_skills_tool(&mut self) -> Result<LLMTool, AgentError> {
        let tool = ListSkillsTool::new(self.skill_registry.clone());
        let llm_tool = tool.to_llm_tool();

        self.runtime
            .block_on(async {
                self.controller
                    .tool_registry()
                    .register(Arc::new(tool))
                    .await
            })
            .map_err(|e| AgentError::ToolRegistration(e.to_string()))?;

        self.tool_definitions.push(llm_tool.clone());
        tracing::info!("Registered list_skills tool");

        Ok(llm_tool)
    }

    /// Add a custom skill search path.
    ///
    /// Skills are discovered from directories containing SKILL.md files.
    /// By default, `$PWD/.skills/` and `~/.agent-air/skills/` are searched.
    pub fn add_skill_path(&mut self, path: std::path::PathBuf) -> &mut Self {
        self.skill_discovery.add_path(path);
        self
    }

    /// Load skills from configured directories.
    ///
    /// This scans all configured skill paths and registers discovered skills
    /// in the skill registry. Call this after configuring skill paths.
    ///
    /// Returns the number of skills loaded and any errors encountered.
    pub fn load_skills(&mut self) -> (usize, Vec<SkillDiscoveryError>) {
        let results = self.skill_discovery.discover();
        self.register_discovered_skills(results)
    }

    /// Load skills from specific paths (one-shot, doesn't modify default discovery).
    ///
    /// This creates a temporary discovery instance with only the provided paths,
    /// loads skills from them, and registers them in the skill registry.
    /// Unlike `add_skill_path()` + `load_skills()`, this doesn't affect the
    /// default discovery paths used by `reload_skills()`.
    ///
    /// Returns the number of skills loaded and any errors encountered.
    pub fn load_skills_from(
        &self,
        paths: Vec<std::path::PathBuf>,
    ) -> (usize, Vec<SkillDiscoveryError>) {
        let mut discovery = SkillDiscovery::empty();
        for path in paths {
            discovery.add_path(path);
        }

        let results = discovery.discover();
        self.register_discovered_skills(results)
    }

    /// Helper to register discovered skills and collect errors.
    ///
    /// Logs a warning if a skill with the same name already exists (duplicate detection).
    fn register_discovered_skills(
        &self,
        results: Vec<Result<crate::skills::Skill, SkillDiscoveryError>>,
    ) -> (usize, Vec<SkillDiscoveryError>) {
        let mut errors = Vec::new();
        let mut count = 0;

        for result in results {
            match result {
                Ok(skill) => {
                    let skill_name = skill.metadata.name.clone();
                    let skill_path = skill.path.clone();
                    let replaced = self.skill_registry.register(skill);

                    if let Some(old_skill) = replaced {
                        tracing::warn!(
                            skill_name = %skill_name,
                            new_path = %skill_path.display(),
                            old_path = %old_skill.path.display(),
                            "Duplicate skill name detected - replaced existing skill"
                        );
                    }

                    tracing::info!(
                        skill_name = %skill_name,
                        skill_path = %skill_path.display(),
                        "Loaded skill"
                    );
                    count += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        path = %e.path.display(),
                        error = %e.message,
                        "Failed to load skill"
                    );
                    errors.push(e);
                }
            }
        }

        tracing::info!("Loaded {} skill(s)", count);
        (count, errors)
    }

    /// Reload skills from configured directories.
    ///
    /// This re-scans all configured skill paths and updates the registry:
    /// - New skills are added
    /// - Removed skills are unregistered
    /// - Existing skills are re-registered (silently updated)
    ///
    /// Returns information about what changed (added/removed only).
    pub fn reload_skills(&mut self) -> SkillReloadResult {
        let current_names: std::collections::HashSet<String> =
            self.skill_registry.names().into_iter().collect();

        let results = self.skill_discovery.discover();
        let mut discovered_names = std::collections::HashSet::new();
        let mut result = SkillReloadResult::default();

        // Process discovered skills
        for discovery_result in results {
            match discovery_result {
                Ok(skill) => {
                    let name = skill.metadata.name.clone();
                    discovered_names.insert(name.clone());

                    if !current_names.contains(&name) {
                        tracing::info!(skill_name = %name, "Added new skill");
                        result.added.push(name);
                    }
                    self.skill_registry.register(skill);
                }
                Err(e) => {
                    tracing::warn!(
                        path = %e.path.display(),
                        error = %e.message,
                        "Failed to load skill during reload"
                    );
                    result.errors.push(e);
                }
            }
        }

        // Find and remove skills that no longer exist
        for name in &current_names {
            if !discovered_names.contains(name) {
                tracing::info!(skill_name = %name, "Removed skill");
                self.skill_registry.unregister(name);
                result.removed.push(name.clone());
            }
        }

        tracing::info!(
            added = result.added.len(),
            removed = result.removed.len(),
            errors = result.errors.len(),
            "Skills reloaded"
        );

        result
    }

    /// Get skills XML for injection into system prompts.
    ///
    /// Returns an XML string listing all available skills that can be
    /// included in the system prompt to inform the LLM about available capabilities.
    pub fn skills_prompt_xml(&self) -> String {
        self.skill_registry.to_prompt_xml()
    }

    /// Refresh a session's system prompt with current skills.
    ///
    /// This updates the session's system prompt to include the current
    /// `<available_skills>` XML from the skill registry.
    ///
    /// Note: This appends the skills XML to the existing system prompt.
    /// If skills were previously loaded, this may result in duplicate entries.
    pub async fn refresh_session_skills(&self, session_id: i64) -> Result<(), AgentError> {
        let skills_xml = self.skills_prompt_xml();
        if skills_xml.is_empty() {
            return Ok(());
        }

        let session = self
            .controller
            .get_session(session_id)
            .await
            .ok_or(AgentError::SessionNotFound(session_id))?;

        let current_prompt = session.system_prompt().await.unwrap_or_default();

        // Check if skills are already in the prompt to avoid duplicates
        let new_prompt = if current_prompt.contains("<available_skills>") {
            // Replace existing skills section
            replace_skills_section(&current_prompt, &skills_xml)
        } else if current_prompt.is_empty() {
            // No existing prompt, just use skills
            skills_xml
        } else {
            // Append skills section
            format!("{}\n\n{}", current_prompt, skills_xml)
        };

        session.set_system_prompt(new_prompt).await;
        tracing::debug!(session_id, "Refreshed session skills");
        Ok(())
    }
}

/// Replace the <available_skills> section in a system prompt.
fn replace_skills_section(prompt: &str, new_skills_xml: &str) -> String {
    if let Some(start) = prompt.find("<available_skills>")
        && let Some(end) = prompt.find("</available_skills>")
    {
        let end = end + "</available_skills>".len();
        let mut result = String::with_capacity(prompt.len());
        result.push_str(&prompt[..start]);
        result.push_str(new_skills_xml);
        result.push_str(&prompt[end..]);
        return result;
    }
    // Fallback: just append
    format!("{}\n\n{}", prompt, new_skills_xml)
}

/// Converts a ControllerEvent to a UiMessage for the frontend.
///
/// This function maps the internal controller events to UI-friendly messages
/// that can be displayed in any frontend (TUI, web, etc.).
///
/// # Architecture Note
///
/// This function serves as the **intentional integration point** between the
/// controller layer (`ControllerEvent`) and the UI layer (`UiMessage`). It is
/// defined in the agent module because:
/// 1. The agent orchestrates both controller and UI components
/// 2. `UiMessage` is an agent-layer type consumed by frontends
/// 3. The agent owns the responsibility of bridging these layers
///
/// Both `LLMController::send_to_ui()` and `AgentAir` initialization use this
/// function to translate controller events into UI-displayable messages.
pub fn convert_controller_event_to_ui_message(event: ControllerEvent) -> UiMessage {
    match event {
        ControllerEvent::StreamStart { session_id, .. } => {
            // Silent - don't display stream start messages
            UiMessage::System {
                session_id,
                message: String::new(),
            }
        }
        ControllerEvent::TextChunk {
            session_id,
            text,
            turn_id,
        } => UiMessage::TextChunk {
            session_id,
            turn_id,
            text,
            input_tokens: 0,
            output_tokens: 0,
        },
        ControllerEvent::ToolUseStart {
            session_id,
            tool_name,
            turn_id,
            ..
        } => UiMessage::Display {
            session_id,
            turn_id,
            message: format!("Executing tool: {}", tool_name),
        },
        ControllerEvent::ToolUse {
            session_id,
            tool,
            display_name,
            display_title,
            turn_id,
        } => UiMessage::ToolExecuting {
            session_id,
            turn_id,
            tool_use_id: tool.id.clone(),
            display_name: display_name.unwrap_or_else(|| tool.name.clone()),
            display_title: display_title.unwrap_or_default(),
        },
        ControllerEvent::Complete {
            session_id,
            turn_id,
            stop_reason,
        } => UiMessage::Complete {
            session_id,
            turn_id,
            input_tokens: 0,
            output_tokens: 0,
            stop_reason,
        },
        ControllerEvent::Error {
            session_id,
            error,
            turn_id,
        } => UiMessage::Error {
            session_id,
            turn_id,
            error,
        },
        ControllerEvent::TokenUpdate {
            session_id,
            input_tokens,
            output_tokens,
            context_limit,
        } => UiMessage::TokenUpdate {
            session_id,
            turn_id: None,
            input_tokens,
            output_tokens,
            context_limit,
        },
        ControllerEvent::ToolResult {
            session_id,
            tool_use_id,
            status,
            error,
            turn_id,
            ..
        } => UiMessage::ToolCompleted {
            session_id,
            turn_id,
            tool_use_id,
            status,
            error,
        },
        ControllerEvent::CommandComplete {
            session_id,
            command,
            success,
            message,
        } => UiMessage::CommandComplete {
            session_id,
            command,
            success,
            message,
        },
        ControllerEvent::UserInteractionRequired {
            session_id,
            tool_use_id,
            request,
            turn_id,
        } => UiMessage::UserInteractionRequired {
            session_id,
            tool_use_id,
            request,
            turn_id,
        },
        ControllerEvent::PermissionRequired {
            session_id,
            tool_use_id,
            request,
            turn_id,
        } => UiMessage::PermissionRequired {
            session_id,
            tool_use_id,
            request,
            turn_id,
        },
        ControllerEvent::BatchPermissionRequired {
            session_id,
            batch,
            turn_id,
        } => UiMessage::BatchPermissionRequired {
            session_id,
            batch,
            turn_id,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::TurnId;

    #[test]
    fn test_convert_text_chunk_event() {
        let event = ControllerEvent::TextChunk {
            session_id: 1,
            text: "Hello".to_string(),
            turn_id: Some(TurnId::new_user_turn(1)),
        };

        let msg = convert_controller_event_to_ui_message(event);

        match msg {
            UiMessage::TextChunk {
                session_id, text, ..
            } => {
                assert_eq!(session_id, 1);
                assert_eq!(text, "Hello");
            }
            _ => panic!("Expected TextChunk message"),
        }
    }

    #[test]
    fn test_convert_error_event() {
        let event = ControllerEvent::Error {
            session_id: 1,
            error: "Test error".to_string(),
            turn_id: None,
        };

        let msg = convert_controller_event_to_ui_message(event);

        match msg {
            UiMessage::Error {
                session_id, error, ..
            } => {
                assert_eq!(session_id, 1);
                assert_eq!(error, "Test error");
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[test]
    fn test_replace_skills_section_replaces_existing() {
        let prompt = "System prompt.\n\n<available_skills>\n  <skill>old</skill>\n</available_skills>\n\nMore text.";
        let new_xml = "<available_skills>\n  <skill>new</skill>\n</available_skills>";

        let result = replace_skills_section(prompt, new_xml);

        assert!(result.contains("<skill>new</skill>"));
        assert!(!result.contains("<skill>old</skill>"));
        assert!(result.contains("System prompt."));
        assert!(result.contains("More text."));
    }

    #[test]
    fn test_replace_skills_section_no_existing() {
        let prompt = "System prompt without skills.";
        let new_xml = "<available_skills>\n  <skill>new</skill>\n</available_skills>";

        let result = replace_skills_section(prompt, new_xml);

        // Falls back to appending
        assert!(result.contains("System prompt without skills."));
        assert!(result.contains("<skill>new</skill>"));
    }

    #[test]
    fn test_replace_skills_section_malformed_no_closing_tag() {
        let prompt =
            "System prompt.\n\n<available_skills>\n  <skill>old</skill>\n\nNo closing tag.";
        let new_xml = "<available_skills>\n  <skill>new</skill>\n</available_skills>";

        let result = replace_skills_section(prompt, new_xml);

        // Falls back to appending since closing tag is missing
        assert!(result.contains("<skill>old</skill>"));
        assert!(result.contains("<skill>new</skill>"));
    }

    #[test]
    fn test_replace_skills_section_at_end() {
        let prompt =
            "System prompt.\n\n<available_skills>\n  <skill>old</skill>\n</available_skills>";
        let new_xml = "<available_skills>\n  <skill>new</skill>\n</available_skills>";

        let result = replace_skills_section(prompt, new_xml);

        assert!(result.contains("<skill>new</skill>"));
        assert!(!result.contains("<skill>old</skill>"));
        assert!(result.starts_with("System prompt."));
    }
}
