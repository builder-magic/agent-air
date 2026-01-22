// AgentCore - Complete working agent out of the box
//
// Users call `AgentCore::new(config)` then `core.run()` and get a working chat agent.

use std::io;
use std::sync::Arc;

use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::controller::{
    ControllerEvent, ControllerInputPayload, LLMController, LLMSessionConfig, LLMTool,
    PermissionRegistry, ToolRegistry, UserInteractionRegistry,
};

use super::config::{load_config, AgentConfig, LLMRegistry};
use super::error::AgentError;
use super::logger::Logger;
use super::messages::channels::DEFAULT_CHANNEL_SIZE;
use super::messages::UiMessage;
use super::router::InputRouter;

use crate::tui::{App, AppConfig, DefaultKeyHandler, ExitHandler, KeyBindings, KeyHandler, LayoutTemplate, SessionInfo};
use crate::tui::widgets::{Widget, ConversationView, ConversationViewFactory};

/// Sender for messages from TUI to controller
pub type ToControllerTx = mpsc::Sender<ControllerInputPayload>;
/// Receiver for messages from TUI to controller
pub type ToControllerRx = mpsc::Receiver<ControllerInputPayload>;
/// Sender for messages from controller to TUI
pub type FromControllerTx = mpsc::Sender<UiMessage>;
/// Receiver for messages from controller to TUI
pub type FromControllerRx = mpsc::Receiver<UiMessage>;

/// AgentCore - A complete, working agent infrastructure.
///
/// AgentCore provides all the infrastructure needed for an LLM-powered agent:
/// - Logging with tracing
/// - LLM configuration loading
/// - Tokio async runtime
/// - LLMController for session management
/// - Communication channels
/// - User interaction and permission registries
///
/// # Basic Usage
///
/// ```ignore
/// struct MyConfig;
/// impl AgentConfig for MyConfig {
///     fn config_path(&self) -> &str { ".myagent/config.yaml" }
///     fn default_system_prompt(&self) -> &str { "You are helpful." }
///     fn log_prefix(&self) -> &str { "myagent" }
///     fn name(&self) -> &str { "MyAgent" }
/// }
///
/// fn main() -> io::Result<()> {
///     let mut core = AgentCore::new(&MyConfig)?;
///     // Access channels and controller to wire up your TUI
///     // then run your TUI loop
///     Ok(())
/// }
/// ```
pub struct AgentCore {
    /// Logger instance (must be kept alive)
    #[allow(dead_code)]
    logger: Logger,

    /// Agent name for display
    name: String,

    /// Agent version for display
    version: String,

    /// Factory for creating conversation views
    conversation_factory: Option<ConversationViewFactory>,

    /// Tokio runtime for async operations
    runtime: Runtime,

    /// The LLM controller
    controller: Arc<LLMController>,

    /// LLM provider registry (loaded from config)
    llm_registry: Option<LLMRegistry>,

    /// Sender for messages from TUI to controller
    to_controller_tx: ToControllerTx,

    /// Receiver for messages from TUI to controller (consumed by InputRouter)
    to_controller_rx: Option<ToControllerRx>,

    /// Sender for messages from controller to TUI (held by event handler)
    #[allow(dead_code)]
    from_controller_tx: FromControllerTx,

    /// Receiver for messages from controller to TUI
    from_controller_rx: Option<FromControllerRx>,

    /// Cancellation token for graceful shutdown
    cancel_token: CancellationToken,

    /// User interaction registry for AskUserQuestions tool
    user_interaction_registry: Arc<UserInteractionRegistry>,

    /// Permission registry for AskForPermissions tool
    permission_registry: Arc<PermissionRegistry>,

    /// Tool definitions to register on sessions
    tool_definitions: Vec<LLMTool>,

    /// Widgets to register with the App
    widgets_to_register: Vec<Box<dyn Widget>>,

    /// Layout template for the TUI
    layout_template: Option<LayoutTemplate>,

    /// Key handler for customizable key bindings
    key_handler: Option<Box<dyn KeyHandler>>,

    /// Exit handler for cleanup before quitting
    exit_handler: Option<Box<dyn ExitHandler>>,

    /// Slash commands (None means use defaults)
    commands: Option<Vec<Box<dyn crate::tui::commands::SlashCommand>>>,

    /// Extension data available to commands
    command_extension: Option<Box<dyn std::any::Any + Send>>,

    /// Custom status bar widget (replaces default if provided)
    custom_status_bar: Option<Box<dyn Widget>>,

    /// Whether to hide the default status bar
    hide_status_bar: bool,
}

impl AgentCore {
    /// Create a new AgentCore with the given configuration.
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
                "No LLM providers configured. Set ANTHROPIC_API_KEY or create ~/{}",
                config.config_path()
            );
        } else {
            tracing::info!(
                "Loaded {} LLM provider(s): {:?}",
                llm_registry.providers().len(),
                llm_registry.providers()
            );
        }

        // Create tokio runtime for async operations
        let runtime = Runtime::new().map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to create runtime: {}", e),
            )
        })?;

        // Create communication channels
        let (to_controller_tx, to_controller_rx) =
            mpsc::channel::<ControllerInputPayload>(DEFAULT_CHANNEL_SIZE);
        let (from_controller_tx, from_controller_rx) =
            mpsc::channel::<UiMessage>(DEFAULT_CHANNEL_SIZE);

        // Create the controller with an event handler that forwards to the UI channel
        let ui_tx = from_controller_tx.clone();
        let event_handler = Box::new(move |event: ControllerEvent| {
            let msg = convert_controller_event_to_ui_message(event);
            // Try to send, log if channel is full (non-blocking to avoid deadlock)
            if let Err(e) = ui_tx.try_send(msg) {
                tracing::warn!("Failed to send controller event to UI: {}", e);
            }
        });

        let controller = Arc::new(LLMController::new(Some(event_handler)));
        let cancel_token = CancellationToken::new();

        // Create channel for user interaction events
        let (interaction_event_tx, mut interaction_event_rx) =
            mpsc::channel::<ControllerEvent>(DEFAULT_CHANNEL_SIZE);

        // Create the user interaction registry
        let user_interaction_registry =
            Arc::new(UserInteractionRegistry::new(interaction_event_tx));

        // Spawn a task to forward user interaction events to the UI channel
        let ui_tx_for_interactions = from_controller_tx.clone();
        runtime.spawn(async move {
            while let Some(event) = interaction_event_rx.recv().await {
                let msg = convert_controller_event_to_ui_message(event);
                if let Err(e) = ui_tx_for_interactions.try_send(msg) {
                    tracing::warn!("Failed to send user interaction event to UI: {}", e);
                }
            }
        });

        // Create channel for permission events
        let (permission_event_tx, mut permission_event_rx) =
            mpsc::channel::<ControllerEvent>(DEFAULT_CHANNEL_SIZE);

        // Create the permission registry
        let permission_registry = Arc::new(PermissionRegistry::new(permission_event_tx));

        // Spawn a task to forward permission events to the UI channel
        let ui_tx_for_permissions = from_controller_tx.clone();
        runtime.spawn(async move {
            while let Some(event) = permission_event_rx.recv().await {
                let msg = convert_controller_event_to_ui_message(event);
                if let Err(e) = ui_tx_for_permissions.try_send(msg) {
                    tracing::warn!("Failed to send permission event to UI: {}", e);
                }
            }
        });

        Ok(Self {
            logger,
            name: config.name().to_string(),
            version: "0.1.0".to_string(),
            conversation_factory: None,
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
            widgets_to_register: Vec::new(),
            layout_template: None,
            key_handler: None,
            exit_handler: None,
            commands: None,
            command_extension: None,
            custom_status_bar: None,
            hide_status_bar: false,
        })
    }

    /// Set the agent version for display.
    pub fn set_version(&mut self, version: impl Into<String>) {
        self.version = version.into();
    }

    /// Set the conversation view factory.
    ///
    /// The factory is called to create conversation views when sessions
    /// are created or cleared. This allows customizing the chat view
    /// with custom welcome screens, title renderers, etc.
    ///
    /// # Example
    ///
    /// ```ignore
    /// agent.set_conversation_factory(|| {
    ///     Box::new(ChatView::new()
    ///         .with_title("My Agent")
    ///         .with_initial_content(welcome_renderer))
    /// });
    /// ```
    pub fn set_conversation_factory<F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn() -> Box<dyn ConversationView> + Send + Sync + 'static,
    {
        self.conversation_factory = Some(Box::new(factory));
        self
    }

    /// Set the layout template for the TUI.
    ///
    /// This allows customizing how widgets are arranged in the terminal.
    /// If not set, the default Standard layout with panels is used.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Use standard layout (default)
    /// agent.set_layout(LayoutTemplate::standard());
    ///
    /// // Add a sidebar
    /// agent.set_layout(LayoutTemplate::with_sidebar("file_browser", 30));
    ///
    /// // Minimal layout (no status bar)
    /// agent.set_layout(LayoutTemplate::minimal());
    /// ```
    pub fn set_layout(&mut self, template: LayoutTemplate) -> &mut Self {
        self.layout_template = Some(template);
        self
    }

    /// Set a custom key handler for the TUI.
    ///
    /// This allows full control over key handling behavior. For simpler
    /// customization where you just want to change which keys trigger
    /// which actions, use [`Self::set_key_bindings`] instead.
    ///
    /// # Example
    ///
    /// ```ignore
    /// struct VimKeyHandler { mode: VimMode }
    /// impl KeyHandler for VimKeyHandler {
    ///     fn handle_key(&mut self, key: KeyEvent, ctx: &KeyContext) -> AppKeyResult {
    ///         // Implement vim-style modal editing
    ///     }
    /// }
    ///
    /// let mut agent = AgentCore::new(&config)?;
    /// agent.set_key_handler(VimKeyHandler { mode: VimMode::Normal });
    /// ```
    pub fn set_key_handler<H: KeyHandler>(&mut self, handler: H) -> &mut Self {
        self.key_handler = Some(Box::new(handler));
        self
    }

    /// Set custom key bindings using the default handler.
    ///
    /// This is a simpler alternative to [`Self::set_key_handler`] when you
    /// only need to change which keys trigger which actions.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Use minimal bindings (Esc to quit, arrow keys only)
    /// let mut agent = AgentCore::new(&config)?;
    /// agent.set_key_bindings(KeyBindings::minimal());
    ///
    /// // Or customize specific bindings
    /// let mut bindings = KeyBindings::emacs();
    /// bindings.quit = vec![KeyCombo::key(KeyCode::Esc)];
    /// agent.set_key_bindings(bindings);
    /// ```
    pub fn set_key_bindings(&mut self, bindings: KeyBindings) -> &mut Self {
        self.key_handler = Some(Box::new(DefaultKeyHandler::new(bindings)));
        self
    }

    /// Set an exit handler for cleanup before quitting.
    ///
    /// The exit handler's `on_exit()` method is called when the user
    /// confirms exit. If it returns `false`, the exit is cancelled.
    ///
    /// # Example
    ///
    /// ```ignore
    /// struct SaveOnExitHandler { session_file: PathBuf }
    /// impl ExitHandler for SaveOnExitHandler {
    ///     fn on_exit(&mut self) -> bool {
    ///         self.save_session();
    ///         true // proceed with exit
    ///     }
    /// }
    ///
    /// let mut agent = AgentCore::new(&config)?;
    /// agent.set_exit_handler(SaveOnExitHandler { session_file: path });
    /// ```
    pub fn set_exit_handler<H: ExitHandler>(&mut self, handler: H) -> &mut Self {
        self.exit_handler = Some(Box::new(handler));
        self
    }

    /// Set the slash commands for this agent.
    ///
    /// If not called, uses the default command set.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use agent_core::tui::commands::{CommandRegistry, CustomCommand, CommandResult};
    ///
    /// agent.set_commands(
    ///     CommandRegistry::with_defaults()
    ///         .add(CustomCommand::new("deploy", "Deploy app", |args, ctx| {
    ///             CommandResult::Message(format!("Deployed to {}", args))
    ///         }))
    ///         .remove("quit")
    ///         .build()
    /// );
    /// ```
    pub fn set_commands(
        &mut self,
        commands: Vec<Box<dyn crate::tui::commands::SlashCommand>>,
    ) -> &mut Self {
        self.commands = Some(commands);
        self
    }

    /// Set extension data available to custom commands.
    ///
    /// Commands can access this via `ctx.extension::<T>()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// struct MyContext { api_key: String }
    ///
    /// agent.set_command_extension(MyContext {
    ///     api_key: "secret".to_string()
    /// });
    /// ```
    pub fn set_command_extension<T: std::any::Any + Send + 'static>(
        &mut self,
        ext: T,
    ) -> &mut Self {
        self.command_extension = Some(Box::new(ext));
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
        let tool_defs = self.runtime.block_on(f(
            self.controller.tool_registry().clone(),
            self.user_interaction_registry.clone(),
            self.permission_registry.clone(),
        ))
        .map_err(AgentError::ToolRegistration)?;
        self.tool_definitions = tool_defs;
        Ok(())
    }

    /// Register a widget with the agent.
    ///
    /// Widgets are registered before calling `run()` and will be available
    /// in the TUI application.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut agent = AgentCore::new(&MyConfig)?;
    /// agent.register_widget(PermissionPanel::new());
    /// agent.register_widget(QuestionPanel::new());
    /// agent.run()
    /// ```
    pub fn register_widget<W: Widget>(&mut self, widget: W) -> &mut Self {
        self.widgets_to_register.push(Box::new(widget));
        self
    }

    /// Set a custom status bar widget to replace the default.
    ///
    /// This will unregister the default status bar and register the custom one.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use agent_core::tui::{StatusBar, StatusBarConfig};
    ///
    /// let mut agent = AgentCore::new(&MyConfig)?;
    /// let custom_status_bar = StatusBar::new()
    ///     .with_renderer(|data, theme| {
    ///         vec![Line::from(format!(" {} | {}", data.model_name, data.session_id))]
    ///     });
    /// agent.set_status_bar(custom_status_bar);
    /// agent.run()
    /// ```
    pub fn set_status_bar<W: Widget>(&mut self, status_bar: W) -> &mut Self {
        self.custom_status_bar = Some(Box::new(status_bar));
        self
    }

    /// Hide the default status bar.
    ///
    /// This will unregister the default status bar widget. Useful for minimal layouts
    /// or when you want to implement your own status display.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut agent = AgentCore::new(&MyConfig)?;
    /// agent.hide_status_bar();
    /// agent.run()
    /// ```
    pub fn hide_status_bar(&mut self) -> &mut Self {
        self.hide_status_bar = true;
        self
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
        config: LLMSessionConfig,
        tools: &[LLMTool],
    ) -> Result<i64, crate::client::error::LlmError> {
        let id = controller.create_session(config).await?;

        // Set tools on the session after creation
        if !tools.is_empty() {
            if let Some(session) = controller.get_session(id).await {
                session.set_tools(tools.to_vec()).await;
            }
        }

        Ok(id)
    }

    /// Create an initial session using the default LLM provider.
    ///
    /// Returns the session ID, model name, and context limit.
    pub fn create_initial_session(&mut self) -> Result<(i64, String, i32), AgentError> {
        let registry = self.llm_registry.as_ref().ok_or_else(|| {
            AgentError::NoConfiguration("No LLM registry available".to_string())
        })?;

        let config = registry.get_default().ok_or_else(|| {
            AgentError::NoConfiguration("No default LLM provider configured".to_string())
        })?;

        let model = config.model.clone();
        let context_limit = config.context_limit;

        let controller = self.controller.clone();
        let tool_definitions = self.tool_definitions.clone();

        let session_id = self.runtime.block_on(Self::create_session_internal(
            &controller,
            config.clone(),
            &tool_definitions,
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

        self.runtime
            .block_on(Self::create_session_internal(
                &controller,
                config,
                &tool_definitions,
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

    /// Run the agent with the default TUI.
    ///
    /// This is the main entry point for running an agent. It:
    /// 1. Starts background tasks (controller, input router)
    /// 2. Creates an App with the configured settings
    /// 3. Wires up all channels and registries
    /// 4. Creates an initial session if LLM providers are configured
    /// 5. Runs the TUI event loop
    /// 6. Shuts down cleanly when the user quits
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn main() -> io::Result<()> {
    ///     let mut agent = AgentCore::new(&MyConfig)?;
    ///     agent.run()
    /// }
    /// ```
    pub fn run(&mut self) -> io::Result<()> {
        tracing::info!("{} starting", self.name);

        // Start background tasks (controller, input router)
        self.start_background_tasks();

        // Create App with our configuration
        let app_config = AppConfig {
            agent_name: self.name.clone(),
            version: self.version.clone(),
            commands: self.commands.take(),
            command_extension: self.command_extension.take(),
            ..Default::default()
        };
        let mut app = App::with_config(app_config);

        // Set conversation factory if provided
        if let Some(factory) = self.conversation_factory.take() {
            app.set_conversation_factory(move || factory());
        }

        // Handle status bar customization
        if self.hide_status_bar {
            // Remove the default status bar
            app.widgets.remove(crate::tui::widgets::widget_ids::STATUS_BAR);
        } else if let Some(custom_status_bar) = self.custom_status_bar.take() {
            // Replace default status bar with custom one
            app.widgets.insert(crate::tui::widgets::widget_ids::STATUS_BAR, custom_status_bar);
        }

        // Register widgets with the App
        for widget in self.widgets_to_register.drain(..) {
            // We need to re-box as the App's register_widget expects impl Widget
            let id = widget.id();
            app.widgets.insert(id, widget);
        }
        app.rebuild_priority_order();

        // Wire up channels, controller, and registries to the App
        app.set_to_controller(self.to_controller_tx.clone());
        if let Some(rx) = self.from_controller_rx.take() {
            app.set_from_controller(rx);
        }
        app.set_controller(self.controller.clone());
        app.set_runtime_handle(self.runtime.handle().clone());
        app.set_user_interaction_registry(self.user_interaction_registry.clone());
        app.set_permission_registry(self.permission_registry.clone());

        // Set layout template if specified
        if let Some(layout) = self.layout_template.take() {
            app.set_layout(layout);
        }

        // Set key handler if specified
        if let Some(handler) = self.key_handler.take() {
            app.set_key_handler_boxed(handler);
        }

        // Set exit handler if specified
        if let Some(handler) = self.exit_handler.take() {
            app.set_exit_handler_boxed(handler);
        }

        // Auto-create session if we have a configured LLM provider
        match self.create_initial_session() {
            Ok((session_id, model, context_limit)) => {
                let session_info = SessionInfo::new(session_id, model.clone(), context_limit);
                app.add_session(session_info);
                app.set_session_id(session_id);
                app.set_model_name(&model);
                app.set_context_limit(context_limit);
                tracing::info!(
                    session_id = session_id,
                    model = %model,
                    "Auto-created session on startup"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "No initial session created");
            }
        }

        // Pass LLM registry to app for creating new sessions
        if let Some(registry) = self.llm_registry.take() {
            app.set_llm_registry(registry);
        }

        // Run the TUI (blocking)
        let result = app.run();

        // Shutdown the agent
        self.shutdown();

        tracing::info!("{} stopped", self.name);
        result
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

    /// Returns a clone of the UI message sender.
    ///
    /// This can be used to send messages to the App's UI event loop.
    pub fn from_controller_tx(&self) -> FromControllerTx {
        self.from_controller_tx.clone()
    }
}

/// Converts a ControllerEvent to a UiMessage for the TUI.
///
/// This function maps the internal controller events to UI-friendly messages
/// that can be displayed in a terminal interface.
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
}
