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
    layout::{Constraint, Direction, Layout, Rect},
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

use super::themes::{render_theme_picker, ThemePickerState};
use super::chat::{ChatView, ChatViewConfig, ToolStatus};
use super::commands::{
    filter_commands, generate_help_message, get_default_commands, is_slash_command, parse_command,
    SlashCommand,
};
use super::input::TextInput;
use super::messages::{different_random_index, random_message_index, FUNNY_MESSAGES};
use super::widgets::{
    PermissionKeyAction, PermissionPanel, QuestionKeyAction, QuestionPanel,
    SessionInfo, SessionPickerState, SlashPopupState, render_session_picker, render_slash_popup,
};
use super::{app_theme, current_theme_name, default_theme_name, get_theme, init_theme};

const PROMPT: &str = " \u{203A} ";
const CONTINUATION_INDENT: &str = "   ";
pub const EXIT_MODE_TIMEOUT_SECS: u64 = 2;

// Pending status messages for chat view spinner
const PENDING_STATUS_TOOLS: &str = "running tools...";
const PENDING_STATUS_LLM: &str = "Processing response from LLM...";

/// Configuration for the App
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Agent name (displayed in title bar)
    pub agent_name: String,
    /// Agent version
    pub version: String,
    /// Welcome ASCII art (displayed when chat is empty)
    pub welcome_art: Vec<String>,
    /// Indices of subtitle lines in welcome_art (for different styling)
    pub welcome_subtitle_indices: Vec<usize>,
    /// Custom slash commands (in addition to defaults)
    pub custom_commands: Vec<SlashCommand>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            agent_name: "Agent".to_string(),
            version: "0.1.0".to_string(),
            welcome_art: vec![
                String::new(),
                "    Type a message to start chatting...".to_string(),
            ],
            welcome_subtitle_indices: vec![1],
            custom_commands: Vec::new(),
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

#[derive(Clone, Copy)]
pub enum AppMode {
    Normal,
    Exit(Instant),
}

pub struct App {
    /// App configuration
    config: AppConfig,

    /// All available commands (defaults + custom)
    commands: Vec<SlashCommand>,

    pub input: TextInput,
    pub mode: AppMode,
    pub should_quit: bool,
    pub chat: ChatView,

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

    /// Current funny message index
    message_index: usize,

    /// When the message was last changed (for rotation)
    last_message_change: Option<Instant>,

    /// Current turn ID we're expecting responses for (to filter stale messages)
    current_turn_id: Option<TurnId>,

    /// Set of currently executing tool IDs (for spinner display)
    executing_tools: HashSet<String>,

    /// Slash command popup state
    pub slash_popup: SlashPopupState,

    /// Filtered slash commands for the popup
    pub filtered_commands: Vec<&'static SlashCommand>,

    /// Theme picker state
    pub theme_picker: ThemePickerState,

    /// Session picker state
    pub session_picker: SessionPickerState,

    /// Question panel state (for AskUserQuestions tool)
    pub question_panel: QuestionPanel,

    /// Permission panel state (for AskForPermissions tool)
    pub permission_panel: PermissionPanel,

    /// List of all sessions created in this instance
    sessions: Vec<SessionInfo>,

    /// Chat views per session (for preserving history when switching)
    chat_views: HashMap<i64, ChatView>,

    /// Custom throbber message (overrides FUNNY_MESSAGES when set)
    custom_throbber_message: Option<String>,

    /// User interaction registry for responding to AskUserQuestions
    user_interaction_registry: Option<Arc<UserInteractionRegistry>>,

    /// Permission registry for responding to AskForPermissions
    permission_registry: Option<Arc<PermissionRegistry>>,
}

impl App {
    pub fn new() -> Self {
        Self::with_config(AppConfig::default())
    }

    pub fn with_config(config: AppConfig) -> Self {
        // Initialize default theme
        let theme_name = default_theme_name();
        if let Some(theme) = get_theme(theme_name) {
            init_theme(theme_name, theme);
        }

        // Build chat view config from app config
        let chat_config = ChatViewConfig {
            agent_name: config.agent_name.clone(),
            welcome_art: config.welcome_art.clone(),
            welcome_subtitle_indices: config.welcome_subtitle_indices.clone(),
        };

        // Combine default commands with custom commands
        let mut commands: Vec<SlashCommand> = get_default_commands().to_vec();
        commands.extend(config.custom_commands.clone());

        Self {
            config,
            commands,
            input: TextInput::new(),
            mode: AppMode::Normal,
            should_quit: false,
            chat: ChatView::with_config(chat_config),
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
            message_index: random_message_index(),
            last_message_change: None,
            current_turn_id: None,
            executing_tools: HashSet::new(),
            slash_popup: SlashPopupState::new(),
            filtered_commands: Vec::new(),
            theme_picker: ThemePickerState::new(),
            session_picker: SessionPickerState::new(),
            question_panel: QuestionPanel::new(),
            permission_panel: PermissionPanel::new(),
            sessions: Vec::new(),
            chat_views: HashMap::new(),
            custom_throbber_message: None,
            user_interaction_registry: None,
            permission_registry: None,
        }
    }

    /// Get the agent name
    pub fn agent_name(&self) -> &str {
        &self.config.agent_name
    }

    /// Get the agent version
    pub fn version(&self) -> &str {
        &self.config.version
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

    pub fn submit_message(&mut self) {
        let content = self.input.take();
        if content.trim().is_empty() {
            return;
        }

        // Check if this is a slash command
        if is_slash_command(&content) {
            self.execute_command(&content);
            return;
        }

        // Add user message to chat and re-enable auto-scroll (user wants to see response)
        self.chat.enable_auto_scroll();
        self.chat.add_user_message(content.clone());

        // Check if we have an active session
        if self.session_id == 0 {
            self.chat.add_system_message(
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
                self.chat
                    .add_system_message("Failed to send message to controller".to_string());
            } else {
                // Immediately show throbber (before streaming starts)
                self.waiting_for_response = true;
                self.waiting_started = Some(Instant::now());
                // Pick a random funny message and start rotation timer
                self.message_index = random_message_index();
                self.last_message_change = Some(Instant::now());
                // Track the expected turn ID to filter stale messages
                self.current_turn_id = Some(TurnId::new_user_turn(self.user_turn_counter));
            }
        }
    }

    /// Interrupt the current LLM request
    pub fn interrupt_request(&mut self) {
        // Only interrupt if we're actually waiting/streaming/executing tools
        if !self.waiting_for_response
            && !self.chat.is_streaming()
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
                self.last_message_change = None;
                self.executing_tools.clear();
                // Keep partial streaming content visible (save it as a message)
                self.chat.complete_streaming();
                // Clear turn ID so any stale messages from this turn are ignored
                self.current_turn_id = None;
                self.chat.add_system_message("Request cancelled".to_string());
            }
        }
    }

    /// Execute a slash command
    fn execute_command(&mut self, input: &str) {
        let Some((cmd_name, _args)) = parse_command(input) else {
            self.chat
                .add_system_message("Invalid command format".to_string());
            return;
        };

        // Handle commands that don't produce a message or handle their own output
        match cmd_name {
            "themes" => {
                self.cmd_themes();
                return;
            }
            "sessions" => {
                self.cmd_sessions();
                return;
            }
            "clear" => {
                self.cmd_clear();
                return;
            }
            "compact" => {
                self.cmd_compact();
                return;
            }
            _ => {}
        }

        let result = match cmd_name {
            "help" => self.cmd_help(),
            "status" => self.cmd_status(),
            "new-session" => self.cmd_new_session(),
            "quit" => self.cmd_quit(),
            "version" => self.cmd_version(),
            _ => format!("Unknown command: /{}", cmd_name),
        };

        self.chat.add_system_message(result);
    }

    fn cmd_help(&self) -> String {
        generate_help_message(&self.commands)
    }

    fn cmd_clear(&mut self) {
        // Create new chat view with same config
        let chat_config = ChatViewConfig {
            agent_name: self.config.agent_name.clone(),
            welcome_art: self.config.welcome_art.clone(),
            welcome_subtitle_indices: self.config.welcome_subtitle_indices.clone(),
        };
        self.chat = ChatView::with_config(chat_config);
        self.user_turn_counter = 0;

        // Send Clear command to controller to clear session conversation
        if self.session_id != 0 {
            if let Some(ref tx) = self.to_controller {
                let payload =
                    ControllerInputPayload::control(self.session_id, ControlCmd::Clear);
                let _ = tx.try_send(payload);
            }
        }
    }

    fn cmd_compact(&mut self) {
        // Check if we have an active session
        if self.session_id == 0 {
            self.chat
                .add_system_message("No active session to compact".to_string());
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
                self.chat
                    .add_system_message("Failed to send compact command".to_string());
            }
        }
    }

    fn cmd_status(&self) -> String {
        if self.session_id == 0 {
            return "No active session".to_string();
        }

        format!(
            "Session Status\n  ID: {}\n  Model: {}",
            self.session_id, self.model_name
        )
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

        // Save current chat view before switching to new session
        if self.session_id != 0 {
            let chat_config = ChatViewConfig {
                agent_name: self.config.agent_name.clone(),
                welcome_art: self.config.welcome_art.clone(),
                welcome_subtitle_indices: self.config.welcome_subtitle_indices.clone(),
            };
            let old_chat = std::mem::replace(&mut self.chat, ChatView::with_config(chat_config));
            self.chat_views.insert(self.session_id, old_chat);
        } else {
            // No previous session, just create new chat
            let chat_config = ChatViewConfig {
                agent_name: self.config.agent_name.clone(),
                welcome_art: self.config.welcome_art.clone(),
                welcome_subtitle_indices: self.config.welcome_subtitle_indices.clone(),
            };
            self.chat = ChatView::with_config(chat_config);
        }

        self.session_id = session_id;
        self.model_name = model.clone();
        self.context_limit = context_limit;
        self.context_used = 0;
        self.user_turn_counter = 0;

        // Return empty string - no system message needed
        String::new()
    }

    fn cmd_quit(&mut self) -> String {
        self.should_quit = true;
        "Goodbye!".to_string()
    }

    fn cmd_version(&self) -> String {
        format!("{} v{}", self.config.agent_name, self.config.version)
    }

    fn cmd_themes(&mut self) {
        let current_name = current_theme_name();
        let current_theme = app_theme();
        self.theme_picker.activate(&current_name, current_theme);
    }

    fn cmd_sessions(&mut self) {
        // Update context_used for current session before displaying
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == self.session_id) {
            session.context_used = self.context_used;
        }

        self.session_picker
            .activate(self.sessions.clone(), self.session_id);
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

        // Save current chat view
        let chat_config = ChatViewConfig {
            agent_name: self.config.agent_name.clone(),
            welcome_art: self.config.welcome_art.clone(),
            welcome_subtitle_indices: self.config.welcome_subtitle_indices.clone(),
        };
        let old_chat = std::mem::replace(&mut self.chat, ChatView::with_config(chat_config));
        self.chat_views.insert(self.session_id, old_chat);

        // Find the target session
        if let Some(session) = self.sessions.iter().find(|s| s.id == session_id) {
            self.session_id = session_id;
            self.model_name = session.model.clone();
            self.context_used = session.context_used;
            self.context_limit = session.context_limit;
            self.user_turn_counter = 0;

            // Restore chat view for this session, or use new empty one
            if let Some(chat) = self.chat_views.remove(&session_id) {
                self.chat = chat;
            }
        }
    }

    /// Add a session to the sessions list
    pub fn add_session(&mut self, info: SessionInfo) {
        self.sessions.push(info);
    }

    /// Submit the question panel response
    pub fn submit_question_panel(&mut self) {
        if !self.question_panel.is_active() {
            return;
        }

        let response = self.question_panel.build_response();
        let tool_use_id = self.question_panel.tool_use_id().to_string();

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

        self.question_panel.deactivate();
    }

    /// Cancel the question panel (closes without responding, tool will get an error)
    pub fn cancel_question_panel(&mut self) {
        if !self.question_panel.is_active() {
            return;
        }

        let tool_use_id = self.question_panel.tool_use_id().to_string();

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

        self.question_panel.deactivate();
    }

    /// Submit the permission panel response
    pub fn submit_permission_panel(&mut self, response: PermissionResponse) {
        if !self.permission_panel.is_active() {
            return;
        }

        let tool_use_id = self.permission_panel.tool_use_id().to_string();

        // Respond to the permission request via the registry
        if let (Some(registry), Some(handle)) = (&self.permission_registry, &self.runtime_handle) {
            let registry = registry.clone();
            handle.spawn(async move {
                if let Err(e) = registry.respond(&tool_use_id, response).await {
                    tracing::error!(%tool_use_id, ?e, "Failed to respond to permission request");
                }
            });
        }

        self.permission_panel.deactivate();
    }

    /// Cancel the permission panel (closes without responding, denies permission)
    pub fn cancel_permission_panel(&mut self) {
        if !self.permission_panel.is_active() {
            return;
        }

        let tool_use_id = self.permission_panel.tool_use_id().to_string();

        // Cancel the pending permission via the registry
        if let (Some(registry), Some(handle)) = (&self.permission_registry, &self.runtime_handle) {
            let registry = registry.clone();
            handle.spawn(async move {
                if let Err(e) = registry.cancel(&tool_use_id).await {
                    tracing::warn!(%tool_use_id, ?e, "Failed to cancel permission request");
                }
            });
        }

        self.permission_panel.deactivate();
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
                self.chat.append_streaming(&text);
            }
            UiMessage::Display { message, .. } => {
                self.chat.add_system_message(message);
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

                self.chat.complete_streaming();

                // Only stop waiting if this is NOT a tool_use stop
                if !is_tool_use {
                    self.waiting_for_response = false;
                    self.waiting_started = None;
                    self.last_message_change = None;
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
                self.chat.complete_streaming();
                self.waiting_for_response = false;
                self.waiting_started = None;
                self.last_message_change = None;
                self.current_turn_id = None;
                self.chat.add_system_message(format!("Error: {}", error));
            }
            UiMessage::System { message, .. } => {
                self.chat.add_system_message(message);
            }
            UiMessage::ToolExecuting {
                tool_use_id,
                display_name,
                display_title,
                ..
            } => {
                self.executing_tools.insert(tool_use_id.clone());
                self.chat
                    .add_tool_message(&tool_use_id, &display_name, &display_title);
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
                self.chat.update_tool_status(&tool_use_id, tool_status);
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
                            self.chat.add_system_message(msg);
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
                    self.chat
                        .update_tool_status(&tool_use_id, ToolStatus::WaitingForUser);
                    self.question_panel
                        .activate(tool_use_id, session_id, request, turn_id);
                }
            }
            UiMessage::PermissionRequired {
                session_id,
                tool_use_id,
                request,
                turn_id,
            } => {
                if session_id == self.session_id {
                    self.chat
                        .update_tool_status(&tool_use_id, ToolStatus::WaitingForUser);
                    self.permission_panel
                        .activate(tool_use_id, session_id, request, turn_id);
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
        self.chat.scroll_up();
    }

    pub fn scroll_down(&mut self) {
        self.chat.scroll_down();
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
        // Check for expired exit mode
        if let AppMode::Exit(entered_at) = self.mode {
            if entered_at.elapsed().as_secs() >= EXIT_MODE_TIMEOUT_SECS {
                self.mode = AppMode::Normal;
            }
        }

        // Reset exit mode on any non-Ctrl+D key press
        let is_ctrl_d = key == KeyCode::Char('d') && modifiers.contains(KeyModifiers::CONTROL);
        if self.is_exit_mode_active() && !is_ctrl_d {
            self.mode = AppMode::Normal;
        }

        // When spinner is active, only allow Esc and Ctrl+D
        let is_processing = self.waiting_for_response || self.chat.is_streaming();
        if is_processing && !self.question_panel.is_active() && !self.permission_panel.is_active() {
            match key {
                KeyCode::Esc => {
                    self.interrupt_request();
                    return;
                }
                KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
                    if self.is_exit_mode_active() {
                        self.should_quit = true;
                    } else if self.input.is_empty() {
                        self.mode = AppMode::Exit(Instant::now());
                    }
                    return;
                }
                _ => return,
            }
        }

        // Handle session picker mode
        if self.session_picker.active {
            match key {
                KeyCode::Up => self.session_picker.select_previous(),
                KeyCode::Down => self.session_picker.select_next(),
                KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.session_picker.select_previous()
                }
                KeyCode::Char('n') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.session_picker.select_next()
                }
                KeyCode::Enter => {
                    if let Some(session_id) = self.session_picker.selected_session_id() {
                        self.session_picker.confirm();
                        self.switch_session(session_id);
                    }
                }
                KeyCode::Esc => self.session_picker.cancel(),
                _ => {}
            }
            return;
        }

        // Handle question panel mode
        if self.question_panel.is_active() {
            let key_event = KeyEvent::new(key, modifiers);
            match self.question_panel.handle_key(key_event) {
                QuestionKeyAction::Submitted(_, _) => self.submit_question_panel(),
                QuestionKeyAction::Cancelled(_) => self.cancel_question_panel(),
                QuestionKeyAction::Handled | QuestionKeyAction::NotHandled => {}
            }
            return;
        }

        // Handle permission panel mode
        if self.permission_panel.is_active() {
            let key_event = KeyEvent::new(key, modifiers);
            match self.permission_panel.handle_key(key_event) {
                PermissionKeyAction::Selected(_, response) => self.submit_permission_panel(response),
                PermissionKeyAction::Cancelled(_) => self.cancel_permission_panel(),
                PermissionKeyAction::None => {}
            }
            return;
        }

        // Handle theme picker mode
        if self.theme_picker.active {
            match key {
                KeyCode::Up => self.theme_picker.select_previous(),
                KeyCode::Down => self.theme_picker.select_next(),
                KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.theme_picker.select_previous()
                }
                KeyCode::Char('n') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.theme_picker.select_next()
                }
                KeyCode::Enter => self.theme_picker.confirm(),
                KeyCode::Esc => self.theme_picker.cancel(),
                _ => {}
            }
            return;
        }

        // Handle slash command popup mode
        if self.slash_popup.active {
            match key {
                KeyCode::Up => self.slash_popup.select_previous(),
                KeyCode::Down => self.slash_popup.select_next(),
                KeyCode::Enter => {
                    let selected_idx = self.slash_popup.selected_index;
                    if let Some(cmd) = self.filtered_commands.get(selected_idx) {
                        let cmd_name = cmd.name;
                        self.input.clear();
                        for c in format!("/{}", cmd_name).chars() {
                            self.input.insert_char(c);
                        }
                        self.slash_popup.deactivate();
                        self.filtered_commands.clear();
                        self.submit_message();
                    }
                }
                KeyCode::Esc => {
                    self.input.clear();
                    self.slash_popup.deactivate();
                    self.filtered_commands.clear();
                }
                KeyCode::Backspace => {
                    if self.input.buffer() == "/" {
                        self.input.clear();
                        self.slash_popup.deactivate();
                        self.filtered_commands.clear();
                    } else {
                        self.input.delete_char_before();
                        self.filtered_commands =
                            filter_commands(get_default_commands(), self.input.buffer());
                        self.slash_popup
                            .set_filtered_count(self.filtered_commands.len());
                    }
                }
                KeyCode::Char(c) => {
                    self.input.insert_char(c);
                    self.filtered_commands =
                        filter_commands(get_default_commands(), self.input.buffer());
                    self.slash_popup
                        .set_filtered_count(self.filtered_commands.len());
                }
                _ => self.slash_popup.deactivate(),
            }
            return;
        }

        match key {
            // Shift+Enter sends Ctrl+J in most terminals
            KeyCode::Char('j') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.insert_char('\n');
            }

            // Cursor movement - Emacs style
            KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => self.input.move_up(),
            KeyCode::Char('n') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.move_down()
            }
            KeyCode::Char('b') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.move_left()
            }
            KeyCode::Char('f') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.move_right()
            }
            KeyCode::Char('a') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.move_to_line_start()
            }
            KeyCode::Char('e') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.move_to_line_end()
            }

            // Editing - Emacs style
            KeyCode::Char('k') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.kill_line()
            }
            KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
                if self.is_exit_mode_active() {
                    self.should_quit = true;
                } else if self.input.is_empty() {
                    self.mode = AppMode::Exit(Instant::now());
                } else {
                    self.input.delete_char_at();
                }
            }

            // Cursor movement - Arrow keys
            KeyCode::Up => self.input.move_up(),
            KeyCode::Down => self.input.move_down(),
            KeyCode::Left => self.input.move_left(),
            KeyCode::Right => self.input.move_right(),
            KeyCode::Home => self.input.move_to_line_start(),
            KeyCode::End => self.input.move_to_line_end(),

            // Text input
            KeyCode::Char(c) => {
                self.input.insert_char(c);
                if c == '/' && self.input.buffer() == "/" {
                    self.slash_popup.activate();
                    self.filtered_commands = filter_commands(get_default_commands(), "/");
                    self.slash_popup
                        .set_filtered_count(self.filtered_commands.len());
                }
            }
            KeyCode::Backspace => self.input.delete_char_before(),
            KeyCode::Delete => self.input.delete_char_at(),
            KeyCode::Enter => self.submit_message(),

            // Escape - interrupt LLM request
            KeyCode::Esc => {
                if self.waiting_for_response || self.chat.is_streaming() {
                    self.interrupt_request();
                }
            }

            _ => {}
        }
    }

    fn is_exit_mode_active(&self) -> bool {
        match self.mode {
            AppMode::Exit(entered_at) => entered_at.elapsed().as_secs() < EXIT_MODE_TIMEOUT_SECS,
            AppMode::Normal => false,
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
                || self.chat.is_streaming()
                || !self.executing_tools.is_empty();

            // Advance animations
            if show_throbber {
                self.animation_frame_counter = self.animation_frame_counter.wrapping_add(1);
                if self.animation_frame_counter % 6 == 0 {
                    self.throbber_state.calc_next();
                    self.chat.step_spinner();
                }

                // Rotate funny message
                if let Some(last_change) = self.last_message_change {
                    let elapsed = last_change.elapsed().as_secs();
                    let interval = 5 + (self.message_index % 11);
                    if elapsed >= interval as u64 {
                        self.message_index = different_random_index(self.message_index);
                        self.last_message_change = Some(Instant::now());
                    }
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
        let frame_width = frame.area().width as usize;
        let frame_height = frame.area().height;

        let slash_popup_active = self.slash_popup.active;
        let question_panel_active = self.question_panel.is_active();
        let permission_panel_active = self.permission_panel.is_active();

        // Calculate input height
        let input_height = if question_panel_active || permission_panel_active {
            0
        } else if show_throbber {
            3
        } else {
            let visual_lines =
                self.input
                    .visual_line_count(frame_width, prompt_len, indent_len);
            (visual_lines as u16) + 2
        };

        // Calculate popup heights
        let slash_popup_height = if slash_popup_active {
            self.slash_popup.popup_height(frame_height)
        } else {
            0
        };

        let question_panel_height = if question_panel_active {
            self.question_panel.panel_height(frame_height)
        } else {
            0
        };

        let permission_panel_height = if permission_panel_active {
            self.permission_panel.panel_height(frame_height)
        } else {
            0
        };

        let interactive_panel_height = if permission_panel_active {
            permission_panel_height
        } else if question_panel_active {
            question_panel_height
        } else {
            0
        };

        let interactive_panel_active = permission_panel_active || question_panel_active;

        // Build layout
        let (chat_area, interactive_panel_area, popup_area, input_area, status_area) =
            match (interactive_panel_active, slash_popup_active) {
                (true, true) => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Min(1),
                            Constraint::Length(interactive_panel_height),
                            Constraint::Length(slash_popup_height),
                            Constraint::Length(input_height),
                            Constraint::Length(2),
                        ])
                        .split(frame.area());
                    (
                        chunks[0],
                        Some(chunks[1]),
                        Some(chunks[2]),
                        chunks[3],
                        chunks[4],
                    )
                }
                (true, false) => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Min(1),
                            Constraint::Length(interactive_panel_height),
                            Constraint::Length(input_height),
                            Constraint::Length(2),
                        ])
                        .split(frame.area());
                    (chunks[0], Some(chunks[1]), None, chunks[2], chunks[3])
                }
                (false, true) => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Min(1),
                            Constraint::Length(slash_popup_height),
                            Constraint::Length(input_height),
                            Constraint::Length(2),
                        ])
                        .split(frame.area());
                    (chunks[0], None, Some(chunks[1]), chunks[2], chunks[3])
                }
                (false, false) => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Min(1),
                            Constraint::Length(input_height),
                            Constraint::Length(2),
                        ])
                        .split(frame.area());
                    (chunks[0], None, None, chunks[1], chunks[2])
                }
            };

        // Render chat
        let pending_status: Option<&str> = if !self.executing_tools.is_empty() {
            Some(PENDING_STATUS_TOOLS)
        } else if self.waiting_for_response && !self.chat.is_streaming() {
            Some(PENDING_STATUS_LLM)
        } else {
            None
        };

        self.chat.render(frame, chat_area, pending_status);

        // Render interactive panel
        if let Some(panel_area) = interactive_panel_area {
            if permission_panel_active {
                self.permission_panel
                    .render(frame, panel_area, &app_theme());
            } else if question_panel_active {
                self.question_panel.render(frame, panel_area, &app_theme());
            }
        }

        // Render slash command popup
        if let Some(popup) = popup_area {
            render_slash_popup(
                &self.slash_popup,
                &self.filtered_commands,
                frame,
                popup,
                &app_theme(),
            );
        }

        // Render input or throbber
        if !question_panel_active && !permission_panel_active {
            if show_throbber {
                let message = self
                    .custom_throbber_message
                    .as_deref()
                    .unwrap_or(FUNNY_MESSAGES[self.message_index]);
                let throbber = Throbber::default()
                    .label(message)
                    .style(app_theme().throbber_label)
                    .throbber_style(app_theme().throbber_spinner)
                    .throbber_set(BRAILLE_EIGHT_DOUBLE);

                let throbber_block = Block::default()
                    .borders(Borders::TOP | Borders::BOTTOM)
                    .border_style(app_theme().input_border);
                let inner = throbber_block.inner(input_area);
                let throbber_inner = Rect::new(
                    inner.x + 1,
                    inner.y,
                    inner.width.saturating_sub(1),
                    inner.height,
                );
                frame.render_widget(throbber_block, input_area);
                frame.render_stateful_widget(throbber, throbber_inner, &mut self.throbber_state);
            } else {
                let input_lines: Vec<String> = self
                    .input
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
                            .border_style(app_theme().input_border),
                    )
                    .wrap(Wrap { trim: false });
                frame.render_widget(input_box, input_area);

                if !self.theme_picker.active {
                    let (cursor_rel_x, cursor_rel_y) = self
                        .input
                        .cursor_display_position_wrapped(frame_width, prompt_len, indent_len);
                    let cursor_x = input_area.x + cursor_rel_x;
                    let cursor_y = input_area.y + 1 + cursor_rel_y;
                    frame.set_cursor_position((cursor_x, cursor_y));
                }
            }
        }

        // Render status bar
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
        } else if self.is_exit_mode_active() {
            " Press Ctrl-D again to exit".to_string()
        } else if show_throbber {
            let elapsed = self
                .waiting_started
                .map(|start| start.elapsed())
                .unwrap_or_default();
            let elapsed_str = format_elapsed(elapsed);
            format!(" escape to interrupt ({})", elapsed_str)
        } else if self.session_id == 0 {
            " No session - type /new-session to start".to_string()
        } else if self.input.is_empty() {
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
                Span::styled(&cwd_display, app_theme().status_help),
                Span::raw(" ".repeat(line1_padding)),
                Span::styled(&context_str, context_style),
                Span::raw("  "),
                Span::styled(format!("{} ", self.model_name), app_theme().status_model),
            ])
        } else {
            Line::from(vec![
                Span::styled(&cwd_display, app_theme().status_help),
                Span::raw(" ".repeat(line1_padding)),
                Span::styled(format!("{} ", self.model_name), app_theme().status_model),
            ])
        };

        let line2 = Line::from(vec![Span::styled(&help_text, app_theme().status_help)]);

        let status_msg = Paragraph::new(vec![line1, line2]);
        frame.render_widget(status_msg, status_area);

        // Render overlays
        if self.theme_picker.active {
            render_theme_picker(&self.theme_picker, frame, frame.area());
        }

        if self.session_picker.active {
            render_session_picker(&self.session_picker, frame, frame.area(), &app_theme());
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
