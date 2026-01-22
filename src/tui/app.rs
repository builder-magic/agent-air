// The main TUI App for LLM agents
//
// This App provides a complete terminal UI with:
// - Chat view with message history
// - Text input with multi-line support
// - Slash command popup
// - Theme picker
// - Session picker
// - Question and permission panels

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Arc;
use std::time::Instant;

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseEventKind,
    },
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    layout::Rect,
    prelude::CrosstermBackend,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use throbber_widgets_tui::{Throbber, ThrobberState, BRAILLE_EIGHT_DOUBLE};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use crate::agent::{FromControllerRx, LLMRegistry, ToControllerTx, UiMessage};
use crate::controller::{
    ControlCmd, ControllerInputPayload, LLMController, PermissionRegistry, PermissionResponse,
    ToolResultStatus, TurnId, UserInteractionRegistry,
};

use super::layout::{LayoutContext, LayoutTemplate, WidgetSizes};
use super::themes::{render_theme_picker, ThemePickerState};
use super::commands::{
    is_slash_command, parse_command,
    CommandContext, CommandResult, PendingAction, SlashCommand,
};
use super::keys::{AppKeyAction, AppKeyResult, DefaultKeyHandler, ExitHandler, KeyBindings, KeyContext, KeyHandler, NavigationHelper};
use super::widgets::{
    widget_ids, ChatView, TextInput, ToolStatus, SessionInfo, SessionPickerState,
    SlashPopupState, Widget, WidgetAction, WidgetKeyContext, WidgetKeyResult, render_session_picker, render_slash_popup,
    PermissionPanel, QuestionPanel, ConversationView, ConversationViewFactory,
};
use super::{app_theme, current_theme_name, default_theme_name, get_theme, init_theme};

const PROMPT: &str = " \u{203A} ";
const CONTINUATION_INDENT: &str = "   ";

// Pending status messages for chat view spinner
const PENDING_STATUS_TOOLS: &str = "running tools...";
const PENDING_STATUS_LLM: &str = "Processing response from LLM...";

/// Callback for dynamic processing messages (e.g., rotating messages).
/// Called on each render frame when waiting for a response.
pub type ProcessingMessageFn = Arc<dyn Fn() -> String + Send + Sync>;

/// Configuration for the App
pub struct AppConfig {
    /// Agent name (displayed in title bar)
    pub agent_name: String,
    /// Agent version
    pub version: String,
    /// Slash commands (if None, uses default commands)
    pub commands: Option<Vec<Box<dyn SlashCommand>>>,
    /// Extension data available to commands via `ctx.extension::<T>()`
    pub command_extension: Option<Box<dyn std::any::Any + Send>>,
    /// Static message shown while processing (default: "Processing request...")
    pub processing_message: String,
    /// Optional callback for dynamic messages. When set, overrides processing_message.
    /// Use this for rotating messages or context-aware status.
    pub processing_message_fn: Option<ProcessingMessageFn>,
}

impl std::fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppConfig")
            .field("agent_name", &self.agent_name)
            .field("version", &self.version)
            .field("commands", &self.commands.as_ref().map(|c| format!("<{} commands>", c.len())))
            .field("command_extension", &self.command_extension.as_ref().map(|_| "<extension>"))
            .field("processing_message", &self.processing_message)
            .field("processing_message_fn", &self.processing_message_fn.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            agent_name: "Agent".to_string(),
            version: "0.1.0".to_string(),
            commands: None, // Use default commands
            command_extension: None,
            processing_message: "Processing request...".to_string(),
            processing_message_fn: None,
        }
    }
}

/// Format a duration for display (e.g., "2s", "1m 30s", "2h 5m")
fn format_elapsed(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let mins = secs / 60;
        let remaining_secs = secs % 60;
        if remaining_secs == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m {}s", mins, remaining_secs)
        }
    } else {
        let hours = secs / 3600;
        let remaining_mins = (secs % 3600) / 60;
        if remaining_mins == 0 {
            format!("{}h", hours)
        } else {
            format!("{}h {}m", hours, remaining_mins)
        }
    }
}

/// Format token counts for display (e.g., "4.3K", "200K", "850")
fn format_tokens(tokens: i64) -> String {
    if tokens >= 100_000 {
        format!("{}K", tokens / 1000)
    } else if tokens >= 1000 {
        format!("{:.1}K", tokens as f64 / 1000.0)
    } else {
        format!("{}", tokens)
    }
}

/// Terminal UI application with chat, input, and command handling.
///
/// Manages the main event loop, widget rendering, and communication with the controller.
pub struct App {
    /// Agent name
    agent_name: String,

    /// Agent version
    version: String,

    /// All available slash commands
    commands: Vec<Box<dyn SlashCommand>>,

    /// Extension data available to commands
    command_extension: Option<Box<dyn std::any::Any + Send>>,

    /// Processing message
    processing_message: String,

    /// Optional callback for dynamic processing messages
    processing_message_fn: Option<ProcessingMessageFn>,

    /// Whether the application should quit.
    pub should_quit: bool,

    /// Sender for messages to the controller
    to_controller: Option<ToControllerTx>,

    /// Receiver for messages from the controller
    from_controller: Option<FromControllerRx>,

    /// Reference to the controller for session management
    controller: Option<Arc<LLMController>>,

    /// LLM provider registry
    llm_registry: Option<LLMRegistry>,

    /// Tokio runtime handle for async operations
    runtime_handle: Option<Handle>,

    /// Current active session ID
    session_id: i64,

    /// Turn counter for user turns
    user_turn_counter: i64,

    /// Model name for display
    model_name: String,

    /// Current context usage (input tokens)
    context_used: i64,

    /// Context limit for the model
    context_limit: i32,

    /// Throbber animation state for progress indicator
    throbber_state: ThrobberState,

    /// Whether we're waiting for a response (set immediately on submit, before streaming starts)
    pub waiting_for_response: bool,

    /// When we started waiting for a response (for elapsed time display)
    waiting_started: Option<Instant>,

    /// Frame counter for throttling animation speed
    animation_frame_counter: u8,

    /// Current turn ID we're expecting responses for (to filter stale messages)
    current_turn_id: Option<TurnId>,

    /// Set of currently executing tool IDs (for spinner display)
    executing_tools: HashSet<String>,

    /// Registered widgets (keyed by ID)
    pub widgets: HashMap<&'static str, Box<dyn Widget>>,

    /// Cached sorted priority order for key event handling
    pub widget_priority_order: Vec<&'static str>,

    /// Filtered slash commands for the popup (indices into self.commands)
    filtered_command_indices: Vec<usize>,

    /// List of all sessions created in this instance
    sessions: Vec<SessionInfo>,

    /// Session state storage for conversation views (used for session switching)
    session_states: HashMap<i64, Box<dyn std::any::Any + Send>>,

    /// The conversation view (decoupled from ChatView)
    conversation_view: Box<dyn ConversationView>,

    /// Factory for creating new conversation views
    conversation_factory: ConversationViewFactory,

    /// Custom throbber message (overrides processing_message when set)
    custom_throbber_message: Option<String>,

    /// User interaction registry for responding to AskUserQuestions
    user_interaction_registry: Option<Arc<UserInteractionRegistry>>,

    /// Permission registry for responding to AskForPermissions
    permission_registry: Option<Arc<PermissionRegistry>>,

    /// Layout template for widget arrangement
    layout_template: LayoutTemplate,

    /// Key handler for customizable key bindings
    key_handler: Box<dyn KeyHandler>,

    /// Optional exit handler for cleanup before quitting
    exit_handler: Option<Box<dyn ExitHandler>>,
}

impl App {
    /// Create a new App with default configuration.
    pub fn new() -> Self {
        Self::with_config(AppConfig::default())
    }

    /// Create a new App with custom configuration.
    pub fn with_config(config: AppConfig) -> Self {
        use super::commands::default_commands;

        // Initialize default theme
        let theme_name = default_theme_name();
        if let Some(theme) = get_theme(theme_name) {
            init_theme(theme_name, theme);
        }

        // Use provided commands or default commands
        let commands = config.commands.unwrap_or_else(default_commands);

        // Create a default conversation factory (creates basic ChatView)
        let default_factory: ConversationViewFactory = Box::new(|| {
            Box::new(ChatView::new())
        });

        Self {
            agent_name: config.agent_name,
            version: config.version,
            commands,
            command_extension: config.command_extension,
            processing_message: config.processing_message,
            processing_message_fn: config.processing_message_fn,
            should_quit: false,
            to_controller: None,
            from_controller: None,
            controller: None,
            llm_registry: None,
            runtime_handle: None,
            session_id: 0,
            user_turn_counter: 0,
            model_name: "Not connected".to_string(),
            context_used: 0,
            context_limit: 0,
            throbber_state: ThrobberState::default(),
            waiting_for_response: false,
            waiting_started: None,
            animation_frame_counter: 0,
            current_turn_id: None,
            executing_tools: HashSet::new(),
            widgets: HashMap::new(),
            widget_priority_order: Vec::new(),
            filtered_command_indices: Vec::new(),
            sessions: Vec::new(),
            session_states: HashMap::new(),
            conversation_view: (default_factory)(),
            conversation_factory: default_factory,
            custom_throbber_message: None,
            user_interaction_registry: None,
            permission_registry: None,
            layout_template: LayoutTemplate::default(),
            key_handler: Box::new(DefaultKeyHandler::default()),
            exit_handler: None,
        }
    }

    /// Register a widget with the app
    ///
    /// The widget will be stored and used for key handling and rendering.
    /// Widgets are identified by their ID and stored in a priority order.
    pub fn register_widget<W: Widget>(&mut self, widget: W) {
        let id = widget.id();
        self.widgets.insert(id, Box::new(widget));
        self.rebuild_priority_order();
    }

    /// Rebuild the priority order cache after widget registration
    pub fn rebuild_priority_order(&mut self) {
        let mut order: Vec<_> = self.widgets.keys().copied().collect();
        order.sort_by(|a, b| {
            let priority_a = self.widgets.get(a).map(|w| w.priority()).unwrap_or(0);
            let priority_b = self.widgets.get(b).map(|w| w.priority()).unwrap_or(0);
            priority_b.cmp(&priority_a) // Descending order (higher priority first)
        });
        self.widget_priority_order = order;
    }

    /// Get a widget by ID
    pub fn widget<W: Widget + 'static>(&self, id: &str) -> Option<&W> {
        self.widgets.get(id).and_then(|w| w.as_any().downcast_ref::<W>())
    }

    /// Get a widget by ID (mutable)
    pub fn widget_mut<W: Widget + 'static>(&mut self, id: &str) -> Option<&mut W> {
        self.widgets.get_mut(id).and_then(|w| w.as_any_mut().downcast_mut::<W>())
    }

    /// Check if a widget is registered
    pub fn has_widget(&self, id: &str) -> bool {
        self.widgets.contains_key(id)
    }

    /// Check if any registered widget blocks input
    fn any_widget_blocks_input(&self) -> bool {
        self.widgets.values().any(|w| w.is_active() && w.blocks_input())
    }

    /// Set the conversation view factory
    ///
    /// The factory is called to create new conversation views when:
    /// - Creating a new session
    /// - Clearing the current conversation (/clear command)
    ///
    /// # Example
    ///
    /// ```ignore
    /// app.set_conversation_factory(|| {
    ///     Box::new(ChatView::new()
    ///         .with_title("My Agent")
    ///         .with_initial_content(welcome_renderer))
    /// });
    /// ```
    pub fn set_conversation_factory<F>(&mut self, factory: F)
    where
        F: Fn() -> Box<dyn ConversationView> + Send + Sync + 'static,
    {
        self.conversation_factory = Box::new(factory);
        // Replace current conversation view with one from the new factory
        self.conversation_view = (self.conversation_factory)();
    }

    /// Get a reference to the TextInput widget if registered
    fn input(&self) -> Option<&TextInput> {
        self.widget::<TextInput>(widget_ids::TEXT_INPUT)
    }

    /// Get a mutable reference to the TextInput widget if registered
    fn input_mut(&mut self) -> Option<&mut TextInput> {
        self.widget_mut::<TextInput>(widget_ids::TEXT_INPUT)
    }

    /// Check if the conversation view is currently streaming
    fn is_chat_streaming(&self) -> bool {
        self.conversation_view.is_streaming()
    }

    /// Get the agent name
    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    /// Get the agent version
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Set the channel for sending messages to the controller
    pub fn set_to_controller(&mut self, tx: ToControllerTx) {
        self.to_controller = Some(tx);
    }

    /// Set the channel for receiving messages from the controller
    pub fn set_from_controller(&mut self, rx: FromControllerRx) {
        self.from_controller = Some(rx);
    }

    /// Set the controller reference for session management
    pub fn set_controller(&mut self, controller: Arc<LLMController>) {
        self.controller = Some(controller);
    }

    /// Set the LLM provider registry
    pub fn set_llm_registry(&mut self, registry: LLMRegistry) {
        self.llm_registry = Some(registry);
    }

    /// Set the tokio runtime handle
    pub fn set_runtime_handle(&mut self, handle: Handle) {
        self.runtime_handle = Some(handle);
    }

    /// Set the user interaction registry
    pub fn set_user_interaction_registry(&mut self, registry: Arc<UserInteractionRegistry>) {
        self.user_interaction_registry = Some(registry);
    }

    /// Set the permission registry
    pub fn set_permission_registry(&mut self, registry: Arc<PermissionRegistry>) {
        self.permission_registry = Some(registry);
    }

    /// Set the session ID
    pub fn set_session_id(&mut self, id: i64) {
        self.session_id = id;
    }

    /// Set the model name
    pub fn set_model_name(&mut self, name: impl Into<String>) {
        self.model_name = name.into();
    }

    /// Set the context limit
    pub fn set_context_limit(&mut self, limit: i32) {
        self.context_limit = limit;
    }

    /// Set the layout template
    pub fn set_layout(&mut self, template: LayoutTemplate) {
        self.layout_template = template;
    }

    /// Set a custom key handler.
    ///
    /// This allows full control over key handling behavior.
    /// For simpler customization, use [`Self::set_key_bindings`] instead.
    pub fn set_key_handler<H: KeyHandler>(&mut self, handler: H) {
        self.key_handler = Box::new(handler);
    }

    /// Set a boxed key handler directly.
    ///
    /// This is useful when you have a `Box<dyn KeyHandler>` already.
    pub fn set_key_handler_boxed(&mut self, handler: Box<dyn KeyHandler>) {
        self.key_handler = handler;
    }

    /// Set custom key bindings using the default handler.
    ///
    /// This is a simpler alternative to [`Self::set_key_handler`] when you
    /// only need to change which keys trigger which actions.
    pub fn set_key_bindings(&mut self, bindings: KeyBindings) {
        self.key_handler = Box::new(DefaultKeyHandler::new(bindings));
    }

    /// Set an exit handler for cleanup before quitting.
    ///
    /// The exit handler's `on_exit()` method is called when the user
    /// confirms exit. If it returns `false`, the exit is cancelled.
    pub fn set_exit_handler<H: ExitHandler>(&mut self, handler: H) {
        self.exit_handler = Some(Box::new(handler));
    }

    /// Set a boxed exit handler directly.
    pub fn set_exit_handler_boxed(&mut self, handler: Box<dyn ExitHandler>) {
        self.exit_handler = Some(handler);
    }

    /// Compute widget sizes for layout computation
    fn compute_widget_sizes(&self, frame_height: u16) -> WidgetSizes {
        let mut heights = HashMap::new();
        let mut is_active = HashMap::new();

        for (id, widget) in &self.widgets {
            heights.insert(*id, widget.required_height(frame_height));
            is_active.insert(*id, widget.is_active());
        }

        WidgetSizes { heights, is_active }
    }

    /// Build the layout context for rendering
    fn build_layout_context<'a>(
        &self,
        frame_area: Rect,
        show_throbber: bool,
        prompt_len: usize,
        indent_len: usize,
        theme: &'a super::themes::Theme,
    ) -> LayoutContext<'a> {
        let frame_width = frame_area.width as usize;
        let input_visual_lines = self
            .input()
            .map(|i| i.visual_line_count(frame_width, prompt_len, indent_len))
            .unwrap_or(1);

        let mut active_widgets = HashSet::new();
        for (id, widget) in &self.widgets {
            if widget.is_active() {
                active_widgets.insert(*id);
            }
        }

        LayoutContext {
            frame_area,
            show_throbber,
            input_visual_lines,
            theme,
            active_widgets,
        }
    }

    pub fn submit_message(&mut self) {
        // Get content from input widget (if registered)
        let content = match self.input_mut() {
            Some(input) => input.take(),
            None => return, // No input widget registered
        };
        if content.trim().is_empty() {
            return;
        }

        // Check if this is a slash command
        if is_slash_command(&content) {
            self.execute_command(&content);
            return;
        }

        // Add user message to chat and re-enable auto-scroll (user wants to see response)
        self.conversation_view.enable_auto_scroll();
        self.conversation_view.add_user_message(content.clone());

        // Check if we have an active session
        if self.session_id == 0 {
            self.conversation_view.add_system_message(
                "No active session. Use /new-session to create one.".to_string(),
            );
            return;
        }

        // Send to controller if channel is available
        if let Some(ref tx) = self.to_controller {
            self.user_turn_counter += 1;
            let turn_id = TurnId::new_user_turn(self.user_turn_counter);
            let payload = ControllerInputPayload::data(self.session_id, content, turn_id);

            // Try to send (non-blocking)
            if tx.try_send(payload).is_err() {
                self.conversation_view.add_system_message("Failed to send message to controller".to_string());
            } else {
                // Immediately show throbber (before streaming starts)
                self.waiting_for_response = true;
                self.waiting_started = Some(Instant::now());
                // Track the expected turn ID to filter stale messages
                self.current_turn_id = Some(TurnId::new_user_turn(self.user_turn_counter));
            }
        }
    }

    /// Interrupt the current LLM request
    pub fn interrupt_request(&mut self) {
        // Only interrupt if we're actually waiting/streaming/executing tools
        if !self.waiting_for_response
            && !self.is_chat_streaming()
            && self.executing_tools.is_empty()
        {
            return;
        }

        // Send interrupt command to controller
        if let Some(ref tx) = self.to_controller {
            let payload = ControllerInputPayload::control(self.session_id, ControlCmd::Interrupt);
            if tx.try_send(payload).is_ok() {
                // Reset waiting state immediately for responsive UI
                self.waiting_for_response = false;
                self.waiting_started = None;
                self.executing_tools.clear();
                // Keep partial streaming content visible (save it as a message)
                self.conversation_view.complete_streaming();
                // Clear turn ID so any stale messages from this turn are ignored
                self.current_turn_id = None;
                self.conversation_view.add_system_message("Request cancelled".to_string());
            }
        }
    }

    /// Execute a slash command using the trait-based system
    fn execute_command(&mut self, input: &str) {
        let Some((cmd_name, args)) = parse_command(input) else {
            self.conversation_view.add_system_message("Invalid command format".to_string());
            return;
        };

        // Find the command by name
        let cmd_idx = self.commands.iter().position(|c| c.name() == cmd_name);
        let Some(cmd_idx) = cmd_idx else {
            self.conversation_view.add_system_message(format!("Unknown command: /{}", cmd_name));
            return;
        };

        // Execute command and collect results (scoped to drop borrows)
        let (result, pending_actions) = {
            // Temporarily take command_extension to avoid borrow issues
            let extension = self.command_extension.take();
            let extension_ref = extension.as_ref().map(|e| e.as_ref() as &dyn std::any::Any);

            let mut ctx = CommandContext::new(
                self.session_id,
                &self.agent_name,
                &self.version,
                &self.commands,
                &mut *self.conversation_view,
                self.to_controller.as_ref(),
                extension_ref,
            );

            // Execute the command
            let result = self.commands[cmd_idx].execute(args, &mut ctx);
            let pending_actions = ctx.take_pending_actions();

            // Restore extension before leaving scope
            self.command_extension = extension;

            (result, pending_actions)
        };

        // Handle pending actions (now that ctx is dropped)
        for action in pending_actions {
            match action {
                PendingAction::OpenThemePicker => self.cmd_themes(),
                PendingAction::OpenSessionPicker => self.cmd_sessions(),
                PendingAction::ClearConversation => self.cmd_clear(),
                PendingAction::CompactConversation => self.cmd_compact(),
                PendingAction::CreateNewSession => { self.cmd_new_session(); }
                PendingAction::Quit => { self.should_quit = true; }
            }
        }

        // Handle result
        match result {
            CommandResult::Ok | CommandResult::Handled => {}
            CommandResult::Message(msg) => {
                self.conversation_view.add_system_message(msg);
            }
            CommandResult::Error(err) => {
                self.conversation_view.add_system_message(format!("Error: {}", err));
            }
            CommandResult::Quit => {
                self.should_quit = true;
            }
        }
    }

    fn cmd_clear(&mut self) {
        // Create a fresh conversation view using the factory
        self.conversation_view = (self.conversation_factory)();
        self.user_turn_counter = 0;

        // Send Clear command to controller to clear session conversation
        if self.session_id != 0 {
            if let Some(ref tx) = self.to_controller {
                let payload =
                    ControllerInputPayload::control(self.session_id, ControlCmd::Clear);
                if let Err(e) = tx.try_send(payload) {
                    tracing::warn!("Failed to send clear command to controller: {}", e);
                }
            }
        }
    }

    fn cmd_compact(&mut self) {
        // Check if we have an active session
        if self.session_id == 0 {
            self.conversation_view.add_system_message("No active session to compact".to_string());
            return;
        }

        // Send Compact command to controller
        if let Some(ref tx) = self.to_controller {
            let payload = ControllerInputPayload::control(self.session_id, ControlCmd::Compact);
            if tx.try_send(payload).is_ok() {
                // Show spinner with "compacting..." message
                self.waiting_for_response = true;
                self.waiting_started = Some(Instant::now());
                self.custom_throbber_message = Some("compacting...".to_string());
            } else {
                self.conversation_view.add_system_message("Failed to send compact command".to_string());
            }
        }
    }

    fn cmd_new_session(&mut self) -> String {
        let Some(ref controller) = self.controller else {
            return "Error: Controller not available".to_string();
        };

        let Some(ref handle) = self.runtime_handle else {
            return "Error: Runtime not available".to_string();
        };

        let Some(ref registry) = self.llm_registry else {
            return "Error: No LLM providers configured.\nSet ANTHROPIC_API_KEY or create config file".to_string();
        };

        // Get the default config from registry
        let Some(config) = registry.get_default() else {
            return "Error: No LLM providers configured.\nSet ANTHROPIC_API_KEY or create config file".to_string();
        };

        let model = config.model.clone();
        let context_limit = config.context_limit;
        let config = config.clone();

        // Create session using the runtime handle
        let controller = controller.clone();
        let session_id = match handle.block_on(async { controller.create_session(config).await }) {
            Ok(id) => id,
            Err(e) => {
                return format!("Error: Failed to create session: {}", e);
            }
        };

        // Add session to the sessions list
        let session_info = SessionInfo::new(session_id, model.clone(), context_limit);
        self.sessions.push(session_info);

        // Save current conversation state before switching to new session
        if self.session_id != 0 {
            let state = self.conversation_view.save_state();
            self.session_states.insert(self.session_id, state);
        }

        // Create new conversation view for the new session
        self.conversation_view = (self.conversation_factory)();

        self.session_id = session_id;
        self.model_name = model.clone();
        self.context_limit = context_limit;
        self.context_used = 0;
        self.user_turn_counter = 0;

        // Return empty string - no system message needed
        String::new()
    }

    fn cmd_themes(&mut self) {
        if let Some(widget) = self.widgets.get_mut(widget_ids::THEME_PICKER) {
            if let Some(picker) = widget.as_any_mut().downcast_mut::<ThemePickerState>() {
                let current_name = current_theme_name();
                let current_theme = app_theme();
                picker.activate(&current_name, current_theme);
            }
        }
    }

    fn cmd_sessions(&mut self) {
        // Update context_used for current session before displaying
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == self.session_id) {
            session.context_used = self.context_used;
        }

        if let Some(widget) = self.widgets.get_mut(widget_ids::SESSION_PICKER) {
            if let Some(picker) = widget.as_any_mut().downcast_mut::<SessionPickerState>() {
                picker.activate(self.sessions.clone(), self.session_id);
            }
        }
    }

    /// Switch to a different session by ID
    pub fn switch_session(&mut self, session_id: i64) {
        // Don't switch if already on this session
        if session_id == self.session_id {
            return;
        }

        // Update context for current session before switching
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == self.session_id) {
            session.context_used = self.context_used;
        }

        // Save current conversation state
        let state = self.conversation_view.save_state();
        self.session_states.insert(self.session_id, state);

        // Find the target session
        if let Some(session) = self.sessions.iter().find(|s| s.id == session_id) {
            self.session_id = session_id;
            self.model_name = session.model.clone();
            self.context_used = session.context_used;
            self.context_limit = session.context_limit;
            self.user_turn_counter = 0;

            // Restore conversation state for this session, or create new one
            if let Some(stored_state) = self.session_states.remove(&session_id) {
                self.conversation_view.restore_state(stored_state);
            } else {
                self.conversation_view = (self.conversation_factory)();
            }
        }
    }

    /// Add a session to the sessions list
    pub fn add_session(&mut self, info: SessionInfo) {
        self.sessions.push(info);
    }

    /// Submit the question panel response
    fn submit_question_panel_response(&mut self, tool_use_id: String, response: crate::controller::AskUserQuestionsResponse) {
        // Respond to the interaction via the registry
        if let (Some(registry), Some(handle)) =
            (&self.user_interaction_registry, &self.runtime_handle)
        {
            let registry = registry.clone();
            handle.spawn(async move {
                if let Err(e) = registry.respond(&tool_use_id, response).await {
                    tracing::error!(%tool_use_id, ?e, "Failed to respond to interaction");
                }
            });
        }

        // Deactivate the widget
        if let Some(widget) = self.widgets.get_mut(widget_ids::QUESTION_PANEL) {
            if let Some(panel) = widget.as_any_mut().downcast_mut::<QuestionPanel>() {
                panel.deactivate();
            }
        }
    }

    /// Cancel the question panel (closes without responding, tool will get an error)
    fn cancel_question_panel_response(&mut self, tool_use_id: String) {
        // Cancel the pending interaction via the registry
        if let (Some(registry), Some(handle)) =
            (&self.user_interaction_registry, &self.runtime_handle)
        {
            let registry = registry.clone();
            handle.spawn(async move {
                if let Err(e) = registry.cancel(&tool_use_id).await {
                    tracing::warn!(%tool_use_id, ?e, "Failed to cancel interaction");
                }
            });
        }

        // Deactivate the widget
        if let Some(widget) = self.widgets.get_mut(widget_ids::QUESTION_PANEL) {
            if let Some(panel) = widget.as_any_mut().downcast_mut::<QuestionPanel>() {
                panel.deactivate();
            }
        }
    }

    /// Submit the permission panel response
    fn submit_permission_panel_response(&mut self, tool_use_id: String, response: PermissionResponse) {
        // Respond to the permission request via the registry
        if let (Some(registry), Some(handle)) = (&self.permission_registry, &self.runtime_handle) {
            let registry = registry.clone();
            handle.spawn(async move {
                if let Err(e) = registry.respond(&tool_use_id, response).await {
                    tracing::error!(%tool_use_id, ?e, "Failed to respond to permission request");
                }
            });
        }

        // Deactivate the widget
        if let Some(widget) = self.widgets.get_mut(widget_ids::PERMISSION_PANEL) {
            if let Some(panel) = widget.as_any_mut().downcast_mut::<PermissionPanel>() {
                panel.deactivate();
            }
        }
    }

    /// Cancel the permission panel (closes without responding, denies permission)
    fn cancel_permission_panel_response(&mut self, tool_use_id: String) {
        // Cancel the pending permission via the registry
        if let (Some(registry), Some(handle)) = (&self.permission_registry, &self.runtime_handle) {
            let registry = registry.clone();
            handle.spawn(async move {
                if let Err(e) = registry.cancel(&tool_use_id).await {
                    tracing::warn!(%tool_use_id, ?e, "Failed to cancel permission request");
                }
            });
        }

        // Deactivate the widget
        if let Some(widget) = self.widgets.get_mut(widget_ids::PERMISSION_PANEL) {
            if let Some(panel) = widget.as_any_mut().downcast_mut::<PermissionPanel>() {
                panel.deactivate();
            }
        }
    }

    /// Process any pending messages from the controller
    fn process_controller_messages(&mut self) {
        // Collect all available messages first to avoid borrow issues
        let mut messages = Vec::new();

        if let Some(ref mut rx) = self.from_controller {
            loop {
                match rx.try_recv() {
                    Ok(msg) => messages.push(msg),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        tracing::warn!("Controller channel disconnected");
                        break;
                    }
                }
            }
        }

        // Process collected messages
        for msg in messages {
            self.handle_ui_message(msg);
        }
    }

    /// Handle a UI message from the controller
    fn handle_ui_message(&mut self, msg: UiMessage) {
        match msg {
            UiMessage::TextChunk { text, turn_id, .. } => {
                // Filter stale messages from cancelled requests
                if !self.is_current_turn(&turn_id) {
                    return;
                }
                self.conversation_view.append_streaming(&text);
            }
            UiMessage::Display { message, .. } => {
                self.conversation_view.add_system_message(message);
            }
            UiMessage::Complete {
                turn_id,
                stop_reason,
                ..
            } => {
                // Filter stale Complete messages from cancelled requests
                if !self.is_current_turn(&turn_id) {
                    return;
                }

                // Check if this is a tool_use stop - if so, tools will execute
                let is_tool_use = stop_reason.as_deref() == Some("tool_use");

                self.conversation_view.complete_streaming();

                // Only stop waiting if this is NOT a tool_use stop
                if !is_tool_use {
                    self.waiting_for_response = false;
                    self.waiting_started = None;
                }
            }
            UiMessage::TokenUpdate {
                input_tokens,
                context_limit,
                ..
            } => {
                self.context_used = input_tokens;
                self.context_limit = context_limit;
            }
            UiMessage::Error { error, turn_id, .. } => {
                if !self.is_current_turn(&turn_id) {
                    return;
                }
                self.conversation_view.complete_streaming();
                self.waiting_for_response = false;
                self.waiting_started = None;
                self.current_turn_id = None;
                self.conversation_view.add_system_message(format!("Error: {}", error));
            }
            UiMessage::System { message, .. } => {
                self.conversation_view.add_system_message(message);
            }
            UiMessage::ToolExecuting {
                tool_use_id,
                display_name,
                display_title,
                ..
            } => {
                self.executing_tools.insert(tool_use_id.clone());
                self.conversation_view.add_tool_message(&tool_use_id, &display_name, &display_title);
            }
            UiMessage::ToolCompleted {
                tool_use_id,
                status,
                error,
                ..
            } => {
                self.executing_tools.remove(&tool_use_id);
                let tool_status = if status == ToolResultStatus::Success {
                    ToolStatus::Completed
                } else {
                    ToolStatus::Failed(error.unwrap_or_default())
                };
                self.conversation_view.update_tool_status(&tool_use_id, tool_status);
            }
            UiMessage::CommandComplete {
                command,
                success,
                message,
                ..
            } => {
                self.waiting_for_response = false;
                self.waiting_started = None;
                self.custom_throbber_message = None;

                match command {
                    ControlCmd::Compact => {
                        if let Some(msg) = message {
                            self.conversation_view.add_system_message(msg);
                        }
                    }
                    ControlCmd::Clear => {}
                    _ => {
                        tracing::debug!(?command, ?success, "Command completed");
                    }
                }
            }
            UiMessage::UserInteractionRequired {
                session_id,
                tool_use_id,
                request,
                turn_id,
            } => {
                if session_id == self.session_id {
                    self.conversation_view.update_tool_status(&tool_use_id, ToolStatus::WaitingForUser);
                    // Activate via widget registry if registered
                    if let Some(widget) = self.widgets.get_mut(widget_ids::QUESTION_PANEL) {
                        if let Some(panel) = widget.as_any_mut().downcast_mut::<QuestionPanel>() {
                            panel.activate(tool_use_id, session_id, request, turn_id);
                        }
                    }
                }
            }
            UiMessage::PermissionRequired {
                session_id,
                tool_use_id,
                request,
                turn_id,
            } => {
                if session_id == self.session_id {
                    self.conversation_view.update_tool_status(&tool_use_id, ToolStatus::WaitingForUser);
                    // Activate via widget registry if registered
                    if let Some(widget) = self.widgets.get_mut(widget_ids::PERMISSION_PANEL) {
                        if let Some(panel) = widget.as_any_mut().downcast_mut::<PermissionPanel>() {
                            panel.activate(tool_use_id, session_id, request, turn_id);
                        }
                    }
                }
            }
        }
    }

    /// Check if the given turn_id matches the current expected turn
    fn is_current_turn(&self, turn_id: &Option<TurnId>) -> bool {
        match (&self.current_turn_id, turn_id) {
            (Some(current), Some(incoming)) => current == incoming,
            (None, _) => false,
            (Some(_), None) => false,
        }
    }

    pub fn scroll_up(&mut self) {
        self.conversation_view.scroll_up();
    }

    pub fn scroll_down(&mut self) {
        self.conversation_view.scroll_down();
    }

    /// Format the context display string for the status bar
    fn format_context_display(&self) -> String {
        if self.context_limit == 0 {
            return String::new();
        }

        let utilization = (self.context_used as f64 / self.context_limit as f64) * 100.0;
        let prefix = if utilization > 80.0 {
            "Context Low:"
        } else {
            "Context:"
        };

        format!(
            "{} {}/{} ({:.0}%)",
            prefix,
            format_tokens(self.context_used),
            format_tokens(self.context_limit as i64),
            utilization
        )
    }

    /// Get the style for context display based on utilization
    fn context_style(&self) -> Style {
        if self.context_limit == 0 {
            return app_theme().status_help;
        }

        let utilization = (self.context_used as f64 / self.context_limit as f64) * 100.0;
        if utilization > 80.0 {
            Style::default().fg(Color::Yellow)
        } else {
            app_theme().status_help
        }
    }

    /// Handle key events
    fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        // Build key context for the handler
        let key_event = KeyEvent::new(key, modifiers);
        let context = KeyContext {
            input_empty: self.input().map(|i| i.is_empty()).unwrap_or(true),
            is_processing: self.waiting_for_response || self.is_chat_streaming(),
            widget_blocking: self.any_widget_blocks_input(),
        };

        // Let the key handler process first
        // (handler manages exit confirmation state internally)
        let result = self.key_handler.handle_key(key_event, &context);

        match result {
            AppKeyResult::Handled => return,
            AppKeyResult::Action(action) => {
                self.execute_key_action(action);
                return;
            }
            AppKeyResult::NotHandled => {
                // Continue to widget dispatch
            }
        }

        // Try to send key event to registered widgets (by priority order)
        let theme = app_theme();
        let nav = NavigationHelper::new(self.key_handler.bindings());
        let widget_ctx = WidgetKeyContext { theme: &theme, nav };

        // Collect widget IDs to check (we need to avoid borrow issues)
        let widget_ids_to_check: Vec<&'static str> = self.widget_priority_order.clone();

        for widget_id in widget_ids_to_check {
            if let Some(widget) = self.widgets.get_mut(widget_id) {
                if widget.is_active() {
                    match widget.handle_key(key_event, &widget_ctx) {
                        WidgetKeyResult::Handled => return,
                        WidgetKeyResult::Action(action) => {
                            self.process_widget_action(action);
                            return;
                        }
                        WidgetKeyResult::NotHandled => {
                            // Continue to next widget or fall through to input handling
                        }
                    }
                }
            }
        }

        // Handle slash popup specially (needs input buffer access)
        if self.is_slash_popup_active() {
            self.handle_slash_popup_key(key);
            return;
        }
    }

    /// Execute an application-level key action.
    fn execute_key_action(&mut self, action: AppKeyAction) {
        match action {
            AppKeyAction::MoveUp => {
                if let Some(input) = self.input_mut() {
                    input.move_up();
                }
            }
            AppKeyAction::MoveDown => {
                if let Some(input) = self.input_mut() {
                    input.move_down();
                }
            }
            AppKeyAction::MoveLeft => {
                if let Some(input) = self.input_mut() {
                    input.move_left();
                }
            }
            AppKeyAction::MoveRight => {
                if let Some(input) = self.input_mut() {
                    input.move_right();
                }
            }
            AppKeyAction::MoveLineStart => {
                if let Some(input) = self.input_mut() {
                    input.move_to_line_start();
                }
            }
            AppKeyAction::MoveLineEnd => {
                if let Some(input) = self.input_mut() {
                    input.move_to_line_end();
                }
            }
            AppKeyAction::DeleteCharBefore => {
                if let Some(input) = self.input_mut() {
                    input.delete_char_before();
                }
            }
            AppKeyAction::DeleteCharAt => {
                if let Some(input) = self.input_mut() {
                    input.delete_char_at();
                }
            }
            AppKeyAction::KillLine => {
                if let Some(input) = self.input_mut() {
                    input.kill_line();
                }
            }
            AppKeyAction::InsertNewline => {
                if let Some(input) = self.input_mut() {
                    input.insert_char('\n');
                }
            }
            AppKeyAction::InsertChar(c) => {
                if let Some(input) = self.input_mut() {
                    input.insert_char(c);
                }
                // Check if we just typed "/" as the first character for slash popup
                if c == '/' && self.input().map(|i| i.buffer() == "/").unwrap_or(false) {
                    self.activate_slash_popup();
                }
            }
            AppKeyAction::Submit => {
                self.submit_message();
            }
            AppKeyAction::Interrupt => {
                self.interrupt_request();
            }
            AppKeyAction::Quit => {
                self.should_quit = true;
            }
            AppKeyAction::RequestExit => {
                // Call exit handler if set, otherwise just quit
                let should_quit = self.exit_handler
                    .as_mut()
                    .map(|h| h.on_exit())
                    .unwrap_or(true);
                if should_quit {
                    self.should_quit = true;
                }
            }
            AppKeyAction::ActivateSlashPopup => {
                self.activate_slash_popup();
            }
            AppKeyAction::Custom(_) => {
                // Custom actions are handled by the custom_action_handler if set.
                // Users implementing custom key bindings should set a handler
                // via with_custom_action_handler() to receive these actions.
            }
        }
    }

    /// Process an action returned by a widget
    fn process_widget_action(&mut self, action: WidgetAction) {
        match action {
            WidgetAction::SubmitQuestion { tool_use_id, response } => {
                self.submit_question_panel_response(tool_use_id, response);
            }
            WidgetAction::CancelQuestion { tool_use_id } => {
                self.cancel_question_panel_response(tool_use_id);
            }
            WidgetAction::SubmitPermission { tool_use_id, response } => {
                self.submit_permission_panel_response(tool_use_id, response);
            }
            WidgetAction::CancelPermission { tool_use_id } => {
                self.cancel_permission_panel_response(tool_use_id);
            }
            WidgetAction::SwitchSession { session_id } => {
                self.switch_session(session_id);
            }
            WidgetAction::ExecuteCommand { command } => {
                // Handle slash popup command selection
                if command.starts_with("__SLASH_INDEX_") {
                    if let Ok(idx) = command.trim_start_matches("__SLASH_INDEX_").parse::<usize>() {
                        self.execute_slash_command_at_index(idx);
                    }
                } else {
                    self.execute_command(&command);
                }
            }
            WidgetAction::Close => {
                // Widget closed itself (e.g., theme picker confirm/cancel)
            }
        }
    }

    /// Check if the slash popup is active
    fn is_slash_popup_active(&self) -> bool {
        self.widgets
            .get(widget_ids::SLASH_POPUP)
            .map(|w| w.is_active())
            .unwrap_or(false)
    }

    /// Filter commands by prefix and return indices into self.commands
    fn filter_command_indices(&self, prefix: &str) -> Vec<usize> {
        let search_term = prefix.trim_start_matches('/').to_lowercase();
        self.commands
            .iter()
            .enumerate()
            .filter(|(_, cmd)| cmd.name().to_lowercase().starts_with(&search_term))
            .map(|(i, _)| i)
            .collect()
    }

    /// Activate the slash popup
    fn activate_slash_popup(&mut self) {
        // Filter first to avoid borrow issues
        let indices = self.filter_command_indices("/");
        let count = indices.len();
        self.filtered_command_indices = indices;

        if let Some(widget) = self.widgets.get_mut(widget_ids::SLASH_POPUP) {
            if let Some(popup) = widget.as_any_mut().downcast_mut::<SlashPopupState>() {
                popup.activate();
                popup.set_filtered_count(count);
            }
        }
    }

    /// Handle key events for the slash popup (needs special handling for input buffer)
    fn handle_slash_popup_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Up => {
                if let Some(widget) = self.widgets.get_mut(widget_ids::SLASH_POPUP) {
                    if let Some(popup) = widget.as_any_mut().downcast_mut::<SlashPopupState>() {
                        popup.select_previous();
                    }
                }
            }
            KeyCode::Down => {
                if let Some(widget) = self.widgets.get_mut(widget_ids::SLASH_POPUP) {
                    if let Some(popup) = widget.as_any_mut().downcast_mut::<SlashPopupState>() {
                        popup.select_next();
                    }
                }
            }
            KeyCode::Enter => {
                let selected_idx = self.widgets
                    .get(widget_ids::SLASH_POPUP)
                    .and_then(|w| w.as_any().downcast_ref::<SlashPopupState>())
                    .map(|p| p.selected_index)
                    .unwrap_or(0);
                self.execute_slash_command_at_index(selected_idx);
            }
            KeyCode::Esc => {
                if let Some(input) = self.input_mut() {
                    input.clear();
                }
                if let Some(widget) = self.widgets.get_mut(widget_ids::SLASH_POPUP) {
                    if let Some(popup) = widget.as_any_mut().downcast_mut::<SlashPopupState>() {
                        popup.deactivate();
                    }
                }
                self.filtered_command_indices.clear();
            }
            KeyCode::Backspace => {
                let is_just_slash = self.input().map(|i| i.buffer() == "/").unwrap_or(false);
                if is_just_slash {
                    if let Some(input) = self.input_mut() {
                        input.clear();
                    }
                    if let Some(widget) = self.widgets.get_mut(widget_ids::SLASH_POPUP) {
                        if let Some(popup) = widget.as_any_mut().downcast_mut::<SlashPopupState>() {
                            popup.deactivate();
                        }
                    }
                    self.filtered_command_indices.clear();
                } else {
                    if let Some(input) = self.input_mut() {
                        input.delete_char_before();
                    }
                    let buffer = self.input().map(|i| i.buffer().to_string()).unwrap_or_default();
                    self.filtered_command_indices = self.filter_command_indices(&buffer);
                    if let Some(widget) = self.widgets.get_mut(widget_ids::SLASH_POPUP) {
                        if let Some(popup) = widget.as_any_mut().downcast_mut::<SlashPopupState>() {
                            popup.set_filtered_count(self.filtered_command_indices.len());
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                if let Some(input) = self.input_mut() {
                    input.insert_char(c);
                }
                let buffer = self.input().map(|i| i.buffer().to_string()).unwrap_or_default();
                self.filtered_command_indices = self.filter_command_indices(&buffer);
                if let Some(widget) = self.widgets.get_mut(widget_ids::SLASH_POPUP) {
                    if let Some(popup) = widget.as_any_mut().downcast_mut::<SlashPopupState>() {
                        popup.set_filtered_count(self.filtered_command_indices.len());
                    }
                }
            }
            _ => {
                if let Some(widget) = self.widgets.get_mut(widget_ids::SLASH_POPUP) {
                    if let Some(popup) = widget.as_any_mut().downcast_mut::<SlashPopupState>() {
                        popup.deactivate();
                    }
                }
            }
        }
    }

    /// Execute slash command at the given index in filtered list
    fn execute_slash_command_at_index(&mut self, idx: usize) {
        // Get the command index from the filtered list
        if let Some(&cmd_idx) = self.filtered_command_indices.get(idx) {
            if let Some(cmd) = self.commands.get(cmd_idx) {
                let cmd_name = cmd.name().to_string();
                if let Some(input) = self.input_mut() {
                    input.clear();
                    for c in format!("/{}", cmd_name).chars() {
                        input.insert_char(c);
                    }
                }
                if let Some(widget) = self.widgets.get_mut(widget_ids::SLASH_POPUP) {
                    if let Some(popup) = widget.as_any_mut().downcast_mut::<SlashPopupState>() {
                        popup.deactivate();
                    }
                }
                self.filtered_command_indices.clear();
                self.submit_message();
            }
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        io::stdout().execute(EnableMouseCapture)?;

        let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

        while !self.should_quit {
            self.process_controller_messages();

            let show_throbber = self.waiting_for_response
                || self.is_chat_streaming()
                || !self.executing_tools.is_empty();

            // Advance animations
            if show_throbber {
                self.animation_frame_counter = self.animation_frame_counter.wrapping_add(1);
                if self.animation_frame_counter % 6 == 0 {
                    self.throbber_state.calc_next();
                    self.conversation_view.step_spinner();
                }
            }

            let prompt_len = PROMPT.chars().count();
            let indent_len = CONTINUATION_INDENT.len();

            terminal.draw(|frame| {
                self.render_frame(frame, show_throbber, prompt_len, indent_len);
            })?;

            // Event handling
            let mut net_scroll: i32 = 0;

            while event::poll(std::time::Duration::from_millis(0))? {
                match event::read()? {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            self.handle_key(key.code, key.modifiers);
                        }
                    }
                    Event::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::ScrollUp => net_scroll -= 1,
                        MouseEventKind::ScrollDown => net_scroll += 1,
                        _ => {}
                    },
                    _ => {}
                }
            }

            // Apply scroll
            if net_scroll < 0 {
                for _ in 0..(-net_scroll) {
                    self.scroll_up();
                }
            } else if net_scroll > 0 {
                for _ in 0..net_scroll {
                    self.scroll_down();
                }
            }

            if net_scroll == 0 {
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }

        io::stdout().execute(DisableMouseCapture)?;
        disable_raw_mode()?;
        io::stdout().execute(LeaveAlternateScreen)?;

        Ok(())
    }

    fn render_frame(
        &mut self,
        frame: &mut ratatui::Frame,
        show_throbber: bool,
        prompt_len: usize,
        indent_len: usize,
    ) {
        let frame_area = frame.area();
        let frame_width = frame_area.width as usize;
        let frame_height = frame_area.height;
        let theme = app_theme();

        // Compute layout using the layout system
        let ctx = self.build_layout_context(frame_area, show_throbber, prompt_len, indent_len, &theme);
        let sizes = self.compute_widget_sizes(frame_height);
        let layout = self.layout_template.compute(&ctx, &sizes);

        // Check overlay widget states (for cursor hiding)
        let theme_picker_active = sizes.is_active(widget_ids::THEME_PICKER);
        let session_picker_active = sizes.is_active(widget_ids::SESSION_PICKER);
        let question_panel_active = sizes.is_active(widget_ids::QUESTION_PANEL);
        let permission_panel_active = sizes.is_active(widget_ids::PERMISSION_PANEL);

        // Render widgets in the order specified by the layout
        for widget_id in &layout.render_order {
            // Skip overlays (rendered last) and special widgets
            if *widget_id == widget_ids::THEME_PICKER || *widget_id == widget_ids::SESSION_PICKER {
                continue;
            }

            // Get the area for this widget
            let Some(area) = layout.widget_areas.get(widget_id) else {
                continue;
            };

            // Handle special widgets that need custom rendering
            match *widget_id {
                id if id == widget_ids::CHAT_VIEW => {
                    // Conversation view is rendered via the trait, not as a widget
                    let pending_status: Option<&str> = if !self.executing_tools.is_empty() {
                        Some(PENDING_STATUS_TOOLS)
                    } else if self.waiting_for_response && !self.is_chat_streaming() {
                        Some(PENDING_STATUS_LLM)
                    } else {
                        None
                    };
                    self.conversation_view.render(frame, *area, &theme, pending_status);
                }
                id if id == widget_ids::TEXT_INPUT => {
                    // Input is rendered specially below (with throbber logic)
                }
                id if id == widget_ids::SLASH_POPUP => {
                    if let Some(widget) = self.widgets.get(widget_ids::SLASH_POPUP) {
                        if let Some(popup_state) = widget.as_any().downcast_ref::<SlashPopupState>() {
                            // Build filtered commands from indices
                            let filtered: Vec<&dyn SlashCommand> = self.filtered_command_indices
                                .iter()
                                .filter_map(|&i| self.commands.get(i).map(|c| c.as_ref()))
                                .collect();
                            render_slash_popup(
                                popup_state,
                                &filtered,
                                frame,
                                *area,
                                &theme,
                            );
                        }
                    }
                }
                _ => {
                    // Generic widget rendering
                    if let Some(widget) = self.widgets.get_mut(widget_id) {
                        if widget.is_active() {
                            widget.render(frame, *area, &theme);
                        }
                    }
                }
            }
        }

        // Render input or throbber (special handling)
        if let Some(input_area) = layout.input_area {
            if !question_panel_active && !permission_panel_active {
                if show_throbber {
                    let default_message;
                    let message = if let Some(msg) = &self.custom_throbber_message {
                        msg.as_str()
                    } else if let Some(ref msg_fn) = self.processing_message_fn {
                        default_message = msg_fn();
                        &default_message
                    } else {
                        &self.processing_message
                    };
                    let throbber = Throbber::default()
                        .label(message)
                        .style(theme.throbber_label)
                        .throbber_style(theme.throbber_spinner)
                        .throbber_set(BRAILLE_EIGHT_DOUBLE);

                    let throbber_block = Block::default()
                        .borders(Borders::TOP | Borders::BOTTOM)
                        .border_style(theme.input_border);
                    let inner = throbber_block.inner(input_area);
                    let throbber_inner = Rect::new(
                        inner.x + 1,
                        inner.y,
                        inner.width.saturating_sub(1),
                        inner.height,
                    );
                    frame.render_widget(throbber_block, input_area);
                    frame.render_stateful_widget(throbber, throbber_inner, &mut self.throbber_state);
                } else if let Some(input) = self.input() {
                    let input_lines: Vec<String> = input
                        .buffer()
                        .split('\n')
                        .enumerate()
                        .map(|(i, line)| {
                            if i == 0 {
                                format!("{}{}", PROMPT, line)
                            } else {
                                format!("{}{}", CONTINUATION_INDENT, line)
                            }
                        })
                        .collect();
                    let input_text = if input_lines.is_empty() {
                        PROMPT.to_string()
                    } else {
                        input_lines.join("\n")
                    };

                    let input_box = Paragraph::new(input_text)
                        .block(
                            Block::default()
                                .borders(Borders::TOP | Borders::BOTTOM)
                                .border_style(theme.input_border),
                        )
                        .wrap(Wrap { trim: false });
                    frame.render_widget(input_box, input_area);

                    // Only show cursor if no overlay is active
                    if !theme_picker_active && !session_picker_active {
                        let (cursor_rel_x, cursor_rel_y) = input
                            .cursor_display_position_wrapped(frame_width, prompt_len, indent_len);
                        let cursor_x = input_area.x + cursor_rel_x;
                        let cursor_y = input_area.y + 1 + cursor_rel_y;
                        frame.set_cursor_position((cursor_x, cursor_y));
                    }
                }
            }
        }

        // Render status bar
        if let Some(status_area) = layout.status_bar_area {
            let cwd = std::env::current_dir()
                .map(|p| {
                    let path_str = p.display().to_string();
                    if let Some(home) = std::env::var_os("HOME") {
                        let home_str = home.to_string_lossy();
                        if path_str.starts_with(home_str.as_ref()) {
                            return format!("~{}", &path_str[home_str.len()..]);
                        }
                    }
                    path_str
                })
                .unwrap_or_else(|_| "unknown".to_string());

            let help_text = if question_panel_active || permission_panel_active {
                String::new()
            } else if let Some(hint) = self.key_handler.status_hint() {
                format!(" {}", hint)
            } else if show_throbber {
                let elapsed = self
                    .waiting_started
                    .map(|start| start.elapsed())
                    .unwrap_or_default();
                let elapsed_str = format_elapsed(elapsed);
                format!(" escape to interrupt ({})", elapsed_str)
            } else if self.session_id == 0 {
                " No session - type /new-session to start".to_string()
            } else if self.input().map(|i| i.is_empty()).unwrap_or(true) {
                " Ctrl-D to exit".to_string()
            } else {
                " Shift-Enter to add a new line".to_string()
            };

            let context_str = self.format_context_display();
            let context_style = self.context_style();

            let status_width = status_area.width as usize;
            let cwd_display = format!(" {}", cwd);
            let cwd_len = cwd_display.chars().count();
            let context_len = context_str.chars().count();
            let model_len = self.model_name.chars().count() + 1;
            let spacing = if context_len > 0 { 2 } else { 0 };
            let total_right = context_len + spacing + model_len;
            let line1_padding = status_width.saturating_sub(cwd_len + total_right);

            let line1 = if context_len > 0 {
                Line::from(vec![
                    Span::styled(&cwd_display, theme.status_help),
                    Span::raw(" ".repeat(line1_padding)),
                    Span::styled(&context_str, context_style),
                    Span::raw("  "),
                    Span::styled(format!("{} ", self.model_name), theme.status_model),
                ])
            } else {
                Line::from(vec![
                    Span::styled(&cwd_display, theme.status_help),
                    Span::raw(" ".repeat(line1_padding)),
                    Span::styled(format!("{} ", self.model_name), theme.status_model),
                ])
            };

            let line2 = Line::from(vec![Span::styled(&help_text, theme.status_help)]);

            let status_msg = Paragraph::new(vec![line1, line2]);
            frame.render_widget(status_msg, status_area);
        }

        // Render overlay widgets (theme picker, session picker) - always on top
        if theme_picker_active {
            if let Some(widget) = self.widgets.get(widget_ids::THEME_PICKER) {
                if let Some(picker) = widget.as_any().downcast_ref::<ThemePickerState>() {
                    render_theme_picker(picker, frame, frame_area);
                }
            }
        }

        if session_picker_active {
            if let Some(widget) = self.widgets.get(widget_ids::SESSION_PICKER) {
                if let Some(picker) = widget.as_any().downcast_ref::<SessionPickerState>() {
                    render_session_picker(picker, frame, frame_area, &theme);
                }
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
