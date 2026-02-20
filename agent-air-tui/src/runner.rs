//! TUI Runner - Provides the run() method for AgentAir
//!
//! This module extends AgentAir with TUI-specific functionality,
//! including widget registration, layout configuration, and the main
//! TUI event loop.

use std::io;

use agent_air_runtime::agent::AgentAir;

use crate::app::{App, AppConfig};
use crate::commands::SlashCommand;
use crate::keys::{DefaultKeyHandler, ExitHandler, KeyBindings, KeyHandler};
use crate::layout::LayoutTemplate;
use crate::widgets::{ConversationView, ConversationViewFactory, SessionInfo, Widget, widget_ids};

/// TUI configuration that extends AgentAir with TUI-specific settings.
///
/// This struct holds all TUI-related configuration that would otherwise
/// need to be stored on AgentAir. Use `TuiRunner::new()` to create one
/// from an AgentAir, configure it, then call `run()`.
pub struct TuiRunner {
    /// The underlying agent air
    agent: AgentAir,

    /// Factory for creating conversation views
    conversation_factory: Option<ConversationViewFactory>,

    /// Widgets to register with the App
    widgets_to_register: Vec<Box<dyn Widget>>,

    /// Layout template for the TUI
    layout_template: Option<LayoutTemplate>,

    /// Key handler for customizable key bindings
    key_handler: Option<Box<dyn KeyHandler>>,

    /// Exit handler for cleanup before quitting
    exit_handler: Option<Box<dyn ExitHandler>>,

    /// Slash commands (None means use defaults)
    commands: Option<Vec<Box<dyn SlashCommand>>>,

    /// Extension data available to commands
    command_extension: Option<Box<dyn std::any::Any + Send>>,

    /// Custom status bar widget (replaces default if provided)
    custom_status_bar: Option<Box<dyn Widget>>,

    /// Whether to hide the default status bar
    hide_status_bar: bool,
}

impl TuiRunner {
    /// Create a new TuiRunner from an AgentAir.
    pub fn new(agent: AgentAir) -> Self {
        Self {
            agent,
            conversation_factory: None,
            widgets_to_register: Vec::new(),
            layout_template: None,
            key_handler: None,
            exit_handler: None,
            commands: None,
            command_extension: None,
            custom_status_bar: None,
            hide_status_bar: false,
        }
    }

    /// Get a mutable reference to the underlying AgentAir.
    pub fn agent_mut(&mut self) -> &mut AgentAir {
        &mut self.agent
    }

    /// Get a reference to the underlying AgentAir.
    pub fn agent(&self) -> &AgentAir {
        &self.agent
    }

    /// Set the conversation view factory.
    ///
    /// The factory is called to create conversation views when sessions
    /// are created or cleared. This allows customizing the chat view
    /// with custom welcome screens, title renderers, etc.
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
    pub fn set_layout(&mut self, template: LayoutTemplate) -> &mut Self {
        self.layout_template = Some(template);
        self
    }

    /// Set a custom key handler for the TUI.
    ///
    /// This allows full control over key handling behavior.
    pub fn set_key_handler<H: KeyHandler>(&mut self, handler: H) -> &mut Self {
        self.key_handler = Some(Box::new(handler));
        self
    }

    /// Set custom key bindings using the default handler.
    ///
    /// This is a simpler alternative to `set_key_handler` when you
    /// only need to change which keys trigger which actions.
    pub fn set_key_bindings(&mut self, bindings: KeyBindings) -> &mut Self {
        self.key_handler = Some(Box::new(DefaultKeyHandler::new(bindings)));
        self
    }

    /// Set an exit handler for cleanup before quitting.
    ///
    /// The exit handler's `on_exit()` method is called when the user
    /// confirms exit. If it returns `false`, the exit is cancelled.
    pub fn set_exit_handler<H: ExitHandler>(&mut self, handler: H) -> &mut Self {
        self.exit_handler = Some(Box::new(handler));
        self
    }

    /// Set the slash commands for this agent.
    ///
    /// If not called, uses the default command set.
    pub fn set_commands(&mut self, commands: Vec<Box<dyn SlashCommand>>) -> &mut Self {
        self.commands = Some(commands);
        self
    }

    /// Set extension data available to custom commands.
    ///
    /// Commands can access this via `ctx.extension::<T>()`.
    pub fn set_command_extension<T: std::any::Any + Send + 'static>(
        &mut self,
        ext: T,
    ) -> &mut Self {
        self.command_extension = Some(Box::new(ext));
        self
    }

    /// Register a widget with the agent.
    ///
    /// Widgets are registered before calling `run()` and will be available
    /// in the TUI application.
    pub fn register_widget<W: Widget>(&mut self, widget: W) -> &mut Self {
        self.widgets_to_register.push(Box::new(widget));
        self
    }

    /// Set a custom status bar widget to replace the default.
    pub fn set_status_bar<W: Widget>(&mut self, status_bar: W) -> &mut Self {
        self.custom_status_bar = Some(Box::new(status_bar));
        self
    }

    /// Hide the default status bar.
    pub fn hide_status_bar(&mut self) -> &mut Self {
        self.hide_status_bar = true;
        self
    }

    /// Run the agent with the TUI.
    ///
    /// This is the main entry point for running an agent with TUI. It:
    /// 1. Starts background tasks (controller, input router)
    /// 2. Creates an App with the configured settings
    /// 3. Wires up all channels and registries
    /// 4. Creates an initial session if LLM providers are configured
    /// 5. Runs the TUI event loop
    /// 6. Shuts down cleanly when the user quits
    pub fn run(mut self) -> io::Result<()> {
        let name = self.agent.name().to_string();
        tracing::info!("{} starting", name);

        // Start background tasks (controller, input router)
        self.agent.start_background_tasks();

        // Create App with our configuration
        let app_config = AppConfig {
            agent_name: name.clone(),
            version: self.agent.version().to_string(),
            commands: self.commands.take(),
            command_extension: self.command_extension.take(),
            error_no_session: self.agent.error_no_session().map(|s| s.to_string()),
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
            app.widgets.remove(widget_ids::STATUS_BAR);
        } else if let Some(custom_status_bar) = self.custom_status_bar.take() {
            // Replace default status bar with custom one
            app.widgets
                .insert(widget_ids::STATUS_BAR, custom_status_bar);
        }

        // Register widgets with the App
        for widget in self.widgets_to_register.drain(..) {
            let id = widget.id();
            app.widgets.insert(id, widget);
        }
        app.rebuild_priority_order();

        // Wire up channels, controller, and registries to the App
        app.set_to_controller(self.agent.to_controller_tx());
        if let Some(rx) = self.agent.take_from_controller_rx() {
            app.set_from_controller(rx);
        }
        app.set_controller(self.agent.controller().clone());
        app.set_runtime_handle(self.agent.runtime_handle());
        app.set_user_interaction_registry(self.agent.user_interaction_registry().clone());
        app.set_permission_registry(self.agent.permission_registry().clone());

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

        // Pass default system prompt to app (needed for onboarding session creation)
        app.set_default_system_prompt(self.agent.default_system_prompt().to_string());

        // Auto-create session if we have a configured LLM provider
        match self.agent.create_initial_session() {
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
        if let Some(registry) = self.agent.take_llm_registry() {
            app.set_llm_registry(registry);
        }

        // Run the TUI (blocking)
        let result = app.run();

        // Shutdown the agent
        self.agent.shutdown();

        tracing::info!("{} stopped", name);
        result
    }
}

/// Extension trait to add TUI functionality to AgentAir.
///
/// This trait is implemented for AgentAir when the `tui` feature is enabled,
/// providing a convenient `into_tui()` method.
pub trait AgentAirExt {
    /// Convert this AgentAir into a TuiRunner for TUI operation.
    fn into_tui(self) -> TuiRunner;
}

impl AgentAirExt for AgentAir {
    fn into_tui(self) -> TuiRunner {
        TuiRunner::new(self)
    }
}
