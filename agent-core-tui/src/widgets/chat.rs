// Chat view widget for displaying messages

use std::collections::HashMap;

use chrono::{DateTime, Local};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph},
};

use crate::themes::theme as app_theme;
use crate::markdown::{render_markdown_with_prefix, wrap_with_prefix};

/// Default configuration values for ChatView
pub mod defaults {
    /// Default prefix for user messages
    pub const USER_PREFIX: &str = "> ";
    /// Default prefix for system messages
    pub const SYSTEM_PREFIX: &str = "* ";
    /// Default prefix for timestamps
    pub const TIMESTAMP_PREFIX: &str = "  - ";
    /// Default continuation line prefix (spaces to align with text after symbol)
    pub const CONTINUATION: &str = "  ";
    /// Default spinner characters for pending status animation
    pub const SPINNER_CHARS: &[char] = &['\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}', '\u{2827}', '\u{2807}', '\u{280F}'];
    /// Default title for the chat view
    pub const DEFAULT_TITLE: &str = "Chat";
    /// Default empty state message
    pub const DEFAULT_EMPTY_MESSAGE: &str = "    Type a message to start chatting...";
    /// Default tool header icon (hammer and pick)
    pub const TOOL_ICON: &str = "\u{2692}";
    /// Default tool executing arrow
    pub const TOOL_EXECUTING_ARROW: &str = "\u{2192}";
    /// Default tool completed checkmark
    pub const TOOL_COMPLETED_CHECKMARK: &str = "\u{2713}";
    /// Default tool failed warning icon
    pub const TOOL_FAILED_ICON: &str = "\u{26A0}";
}

/// Configuration for ChatView widget
///
/// Use the builder pattern to customize the chat view appearance.
///
/// # Example
/// ```rust,ignore
/// let config = ChatViewConfig::new()
///     .with_user_prefix("You: ")
///     .with_spinner_chars(&['|', '/', '-', '\\']);
/// let chat = ChatView::with_config(config);
/// ```
#[derive(Clone)]
pub struct ChatViewConfig {
    /// Prefix for user messages (e.g., "> ")
    pub user_prefix: String,
    /// Prefix for system messages (e.g., "* ")
    pub system_prefix: String,
    /// Prefix for timestamps (e.g., "  - ")
    pub timestamp_prefix: String,
    /// Continuation line prefix for wrapped text
    pub continuation: String,
    /// Spinner characters for pending status animation
    pub spinner_chars: Vec<char>,
    /// Default title for the chat view
    pub default_title: String,
    /// Message shown when chat is empty
    pub empty_message: String,
    /// Icon for tool headers
    pub tool_icon: String,
    /// Arrow for executing tools
    pub tool_executing_arrow: String,
    /// Checkmark for completed tools
    pub tool_completed_checkmark: String,
    /// Warning icon for failed tools
    pub tool_failed_icon: String,
}

impl Default for ChatViewConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatViewConfig {
    /// Create a new ChatViewConfig with default values
    pub fn new() -> Self {
        Self {
            user_prefix: defaults::USER_PREFIX.to_string(),
            system_prefix: defaults::SYSTEM_PREFIX.to_string(),
            timestamp_prefix: defaults::TIMESTAMP_PREFIX.to_string(),
            continuation: defaults::CONTINUATION.to_string(),
            spinner_chars: defaults::SPINNER_CHARS.to_vec(),
            default_title: defaults::DEFAULT_TITLE.to_string(),
            empty_message: defaults::DEFAULT_EMPTY_MESSAGE.to_string(),
            tool_icon: defaults::TOOL_ICON.to_string(),
            tool_executing_arrow: defaults::TOOL_EXECUTING_ARROW.to_string(),
            tool_completed_checkmark: defaults::TOOL_COMPLETED_CHECKMARK.to_string(),
            tool_failed_icon: defaults::TOOL_FAILED_ICON.to_string(),
        }
    }

    /// Set the user message prefix
    pub fn with_user_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.user_prefix = prefix.into();
        self
    }

    /// Set the system message prefix
    pub fn with_system_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.system_prefix = prefix.into();
        self
    }

    /// Set the timestamp prefix
    pub fn with_timestamp_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.timestamp_prefix = prefix.into();
        self
    }

    /// Set the continuation line prefix
    pub fn with_continuation(mut self, continuation: impl Into<String>) -> Self {
        self.continuation = continuation.into();
        self
    }

    /// Set the spinner characters
    pub fn with_spinner_chars(mut self, chars: &[char]) -> Self {
        self.spinner_chars = chars.to_vec();
        self
    }

    /// Set the default title
    pub fn with_default_title(mut self, title: impl Into<String>) -> Self {
        self.default_title = title.into();
        self
    }

    /// Set the empty state message
    pub fn with_empty_message(mut self, message: impl Into<String>) -> Self {
        self.empty_message = message.into();
        self
    }

    /// Set the tool header icon
    pub fn with_tool_icon(mut self, icon: impl Into<String>) -> Self {
        self.tool_icon = icon.into();
        self
    }

    /// Set all tool status icons at once
    pub fn with_tool_status_icons(
        mut self,
        executing_arrow: impl Into<String>,
        completed_checkmark: impl Into<String>,
        failed_icon: impl Into<String>,
    ) -> Self {
        self.tool_executing_arrow = executing_arrow.into();
        self.tool_completed_checkmark = completed_checkmark.into();
        self.tool_failed_icon = failed_icon.into();
        self
    }
}

/// Role of a chat message
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Status of a tool execution
#[derive(Debug, Clone, PartialEq)]
pub enum ToolStatus {
    Executing,
    WaitingForUser,
    Completed,
    Failed(String),
}

/// Data for tool execution messages
/// Display data for tool execution messages.
#[derive(Debug, Clone)]
pub struct ToolMessageData {
    /// Unique identifier for the tool use - stored for debugging even though
    /// primary lookup is done via HashMap key in ChatView.
    #[allow(dead_code)]
    pub tool_use_id: String,
    /// Tool name for display.
    pub display_name: String,
    /// Tool execution title.
    pub display_title: String,
    /// Current execution status.
    pub status: ToolStatus,
}

struct Message {
    role: MessageRole,
    content: String,
    timestamp: DateTime<Local>,
    /// Cached rendered lines for this message
    cached_lines: Option<Vec<Line<'static>>>,
    /// Width at which the cache was generated (invalidate if width changes)
    cached_width: usize,
    /// Tool-specific data (only populated for Tool role)
    tool_data: Option<ToolMessageData>,
}

impl Message {
    fn new(role: MessageRole, content: String) -> Self {
        Self {
            role,
            content,
            timestamp: Local::now(),
            cached_lines: None,
            cached_width: 0,
            tool_data: None,
        }
    }

    fn new_tool(tool_data: ToolMessageData) -> Self {
        Self {
            role: MessageRole::Tool,
            content: String::new(),
            timestamp: Local::now(),
            cached_lines: None,
            cached_width: 0,
            tool_data: Some(tool_data),
        }
    }

    /// Get or render cached lines for this message
    fn get_rendered_lines(&mut self, available_width: usize, config: &ChatViewConfig) -> &[Line<'static>] {
        // Invalidate cache if width changed
        if self.cached_width != available_width {
            self.cached_lines = None;
        }

        // Render and cache if needed
        if self.cached_lines.is_none() {
            let lines = self.render_lines(available_width, config);
            self.cached_lines = Some(lines);
            self.cached_width = available_width;
        }

        self.cached_lines.as_ref().unwrap()
    }

    /// Render this message to lines (called only when cache is invalid)
    fn render_lines(&self, available_width: usize, config: &ChatViewConfig) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let t = app_theme();

        match self.role {
            MessageRole::User => {
                let rendered = wrap_with_prefix(
                    &self.content,
                    &config.user_prefix,
                    t.user_prefix,
                    &config.continuation,
                    available_width,
                    &t,
                );
                lines.extend(rendered);
            }
            MessageRole::System => {
                let rendered = wrap_with_prefix(
                    &self.content,
                    &config.system_prefix,
                    t.system_prefix,
                    &config.continuation,
                    available_width,
                    &t,
                );
                lines.extend(rendered);
            }
            MessageRole::Assistant => {
                let rendered = render_markdown_with_prefix(&self.content, available_width, &t);
                lines.extend(rendered);
            }
            MessageRole::Tool => {
                if let Some(ref data) = self.tool_data {
                    lines.extend(render_tool_message(data, config, available_width));
                }
            }
        }

        // Timestamp line (only for user and system messages)
        // Note: Using %I (with leading zero) instead of %-I for cross-platform compatibility
        if self.role != MessageRole::Assistant && self.role != MessageRole::Tool {
            let time_str = self.timestamp.format("%I:%M:%S %p").to_string();
            let timestamp_text = format!("{}{}", config.timestamp_prefix, time_str);
            lines.push(Line::from(vec![Span::styled(
                timestamp_text,
                app_theme().timestamp,
            )]));
        }

        // Blank line after each message
        lines.push(Line::from(""));

        lines
    }
}

// Re-export RenderFn from chat_helpers for backwards compatibility
pub use super::chat_helpers::RenderFn;

use crate::themes::Theme;

/// Function type for rendering custom title bars
/// Returns (left_title, right_title) as ratatui Lines
pub type TitleRenderFn = Box<dyn Fn(&str, &Theme) -> (Line<'static>, Line<'static>) + Send + Sync>;

/// Chat message display widget with streaming and tool execution support.
pub struct ChatView {
    messages: Vec<Message>,
    scroll_offset: u16,
    /// Buffer for streaming assistant response
    streaming_buffer: Option<String>,
    /// Cached rendered lines for streaming buffer
    streaming_cache: Option<Vec<Line<'static>>>,
    /// Length of streaming_buffer when cache was created
    streaming_cache_len: usize,
    /// Width at which streaming cache was created
    streaming_cache_width: usize,
    /// Cached max scroll value from last render
    last_max_scroll: u16,
    /// Whether auto-scroll is enabled (disabled when user manually scrolls)
    auto_scroll_enabled: bool,
    /// Index for O(1) tool message lookup by tool_use_id
    tool_index: HashMap<String, usize>,
    /// Spinner index for pending status animation
    spinner_index: usize,
    /// Title displayed in the title bar
    title: String,
    /// Optional custom initial content renderer (shown when no messages)
    render_initial_content: Option<RenderFn>,
    /// Optional custom title renderer
    render_title: Option<TitleRenderFn>,
    /// Configuration for display customization
    config: ChatViewConfig,
}

impl ChatView {
    /// Create a new ChatView with default settings
    pub fn new() -> Self {
        Self::with_config(ChatViewConfig::new())
    }

    /// Create a new ChatView with custom configuration
    pub fn with_config(config: ChatViewConfig) -> Self {
        let title = config.default_title.clone();
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            streaming_buffer: None,
            streaming_cache: None,
            streaming_cache_len: 0,
            streaming_cache_width: 0,
            last_max_scroll: 0,
            auto_scroll_enabled: true,
            tool_index: HashMap::new(),
            spinner_index: 0,
            title,
            render_initial_content: None,
            render_title: None,
            config,
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &ChatViewConfig {
        &self.config
    }

    /// Set a new configuration
    pub fn set_config(&mut self, config: ChatViewConfig) {
        self.config = config;
        // Invalidate all message caches since prefixes may have changed
        for msg in &mut self.messages {
            msg.cached_lines = None;
        }
    }

    /// Set the title displayed in the title bar
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set custom initial content renderer (shown when no messages)
    ///
    /// Provide a closure for full ratatui control over what to display
    /// when the chat has no messages yet.
    pub fn with_initial_content(mut self, render: RenderFn) -> Self {
        self.render_initial_content = Some(render);
        self
    }

    /// Set custom title bar renderer
    ///
    /// The function receives the current title and theme, and returns
    /// (left_title, right_title) as ratatui Lines.
    pub fn with_title_renderer<F>(mut self, render: F) -> Self
    where
        F: Fn(&str, &Theme) -> (Line<'static>, Line<'static>) + Send + Sync + 'static,
    {
        self.render_title = Some(Box::new(render));
        self
    }

    /// Set the title displayed in the title bar (mutable setter)
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// Get the current title
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Advance the spinner animation
    pub fn step_spinner(&mut self) {
        let len = self.config.spinner_chars.len().max(1);
        self.spinner_index = (self.spinner_index + 1) % len;
    }

    /// Add a user message (does not force scroll - caller should handle that)
    pub fn add_user_message(&mut self, content: String) {
        if !content.trim().is_empty() {
            self.messages.push(Message::new(MessageRole::User, content));
            // Only scroll to bottom if auto-scroll is enabled
            if self.auto_scroll_enabled {
                self.scroll_offset = u16::MAX;
            }
        }
    }

    /// Add an assistant message (complete)
    pub fn add_assistant_message(&mut self, content: String) {
        if !content.trim().is_empty() {
            self.messages
                .push(Message::new(MessageRole::Assistant, content));
            // Only scroll to bottom if auto-scroll is enabled
            if self.auto_scroll_enabled {
                self.scroll_offset = u16::MAX;
            }
        }
    }

    /// Add a system message (ignores empty messages)
    pub fn add_system_message(&mut self, content: String) {
        if content.trim().is_empty() {
            return;
        }
        self.messages
            .push(Message::new(MessageRole::System, content));
        // Only scroll to bottom if auto-scroll is enabled
        if self.auto_scroll_enabled {
            self.scroll_offset = u16::MAX;
        }
    }

    /// Add a tool execution message
    pub fn add_tool_message(
        &mut self,
        tool_use_id: &str,
        display_name: &str,
        display_title: &str,
    ) {
        let index = self.messages.len();

        let tool_data = ToolMessageData {
            tool_use_id: tool_use_id.to_string(),
            display_name: display_name.to_string(),
            display_title: display_title.to_string(),
            status: ToolStatus::Executing,
        };

        self.messages.push(Message::new_tool(tool_data));
        self.tool_index.insert(tool_use_id.to_string(), index);

        // Only scroll to bottom if auto-scroll is enabled
        if self.auto_scroll_enabled {
            self.scroll_offset = u16::MAX;
        }
    }

    /// Update a tool message status by tool_use_id - O(1) lookup
    pub fn update_tool_status(&mut self, tool_use_id: &str, status: ToolStatus) {
        if let Some(&index) = self.tool_index.get(tool_use_id) {
            if let Some(msg) = self.messages.get_mut(index) {
                if let Some(ref mut data) = msg.tool_data {
                    data.status = status;
                    msg.cached_lines = None; // Invalidate cache
                }
            }
        }
    }

    /// Re-enable auto-scroll and scroll to bottom (call when user submits a message)
    pub fn enable_auto_scroll(&mut self) {
        self.auto_scroll_enabled = true;
        self.scroll_offset = u16::MAX;
    }

    /// Append text to the streaming buffer
    pub fn append_streaming(&mut self, text: &str) {
        match &mut self.streaming_buffer {
            Some(buffer) => buffer.push_str(text),
            None => self.streaming_buffer = Some(text.to_string()),
        }
        // Invalidate streaming cache (buffer content changed)
        self.streaming_cache = None;
        self.streaming_cache_len = 0;
        self.streaming_cache_width = 0;
        // Only auto-scroll if enabled (user hasn't manually scrolled)
        if self.auto_scroll_enabled {
            self.scroll_offset = u16::MAX;
        }
    }

    /// Complete the streaming response and add as assistant message
    pub fn complete_streaming(&mut self) {
        if let Some(content) = self.streaming_buffer.take() {
            if !content.trim().is_empty() {
                self.messages
                    .push(Message::new(MessageRole::Assistant, content));
            }
        }
        // Clear streaming cache
        self.streaming_cache = None;
        self.streaming_cache_len = 0;
        self.streaming_cache_width = 0;
    }

    /// Discard the streaming buffer without saving (used on cancel)
    pub fn discard_streaming(&mut self) {
        self.streaming_buffer = None;
        // Clear streaming cache
        self.streaming_cache = None;
        self.streaming_cache_len = 0;
        self.streaming_cache_width = 0;
    }

    /// Check if currently streaming
    pub fn is_streaming(&self) -> bool {
        self.streaming_buffer.is_some()
    }

    pub fn scroll_up(&mut self) {
        // If at auto-scroll (MAX), convert to actual position first
        if self.scroll_offset == u16::MAX {
            self.scroll_offset = self.last_max_scroll;
        }
        self.scroll_offset = self.scroll_offset.saturating_sub(3);
        // User manually scrolled - disable auto-scroll
        self.auto_scroll_enabled = false;
    }

    pub fn scroll_down(&mut self) {
        // If at auto-scroll (MAX), already at bottom
        if self.scroll_offset == u16::MAX {
            return;
        }
        self.scroll_offset = self.scroll_offset.saturating_add(3);
        // If we've scrolled to bottom, switch back to auto-scroll
        if self.scroll_offset >= self.last_max_scroll {
            self.scroll_offset = u16::MAX;
            self.auto_scroll_enabled = true; // Re-enable when reaching bottom
        }
    }

    pub fn render_chat(&mut self, frame: &mut Frame, area: Rect, pending_status: Option<&str>) {
        let theme = app_theme();

        // Create block - with custom title if renderer provided, plain border otherwise
        let content_block = if let Some(ref render_fn) = self.render_title {
            let (left_title, right_title) = render_fn(&self.title, &theme);
            Block::default()
                .title(left_title)
                .title_alignment(Alignment::Left)
                .title(right_title.alignment(Alignment::Right))
                .borders(Borders::TOP)
                .border_style(theme.border)
                .padding(Padding::new(1, 0, 1, 0))
        } else {
            Block::default()
                .borders(Borders::TOP)
                .border_style(theme.border)
                .padding(Padding::new(1, 0, 1, 0))
        };

        // Check if we're in initial state (no messages yet)
        let is_initial_state = self.messages.is_empty() && self.streaming_buffer.is_none() && pending_status.is_none();

        // If we have custom initial content renderer, use it and return early
        if is_initial_state {
            if let Some(ref render_fn) = self.render_initial_content {
                let inner = content_block.inner(area);
                frame.render_widget(content_block, area);
                render_fn(frame, inner, &theme);
                return;
            }
        }

        // Calculate available width for manual wrapping
        let available_width = area.width.saturating_sub(2) as usize; // -2 for left padding + margin

        // Build message lines using cached rendering
        let mut message_lines: Vec<Line> = Vec::new();

        // Show default initial message if no custom renderer
        if is_initial_state {
            message_lines.push(Line::from(""));
            message_lines.push(Line::from(Span::styled(
                self.config.empty_message.clone(),
                Style::default().fg(Color::DarkGray),
            )));
        }

        for msg in &mut self.messages {
            // Use cached lines (renders only if cache is invalid)
            let cached = msg.get_rendered_lines(available_width, &self.config);
            message_lines.extend(cached.iter().cloned());
        }

        // Add streaming buffer if present (with caching)
        if let Some(ref buffer) = self.streaming_buffer {
            let buffer_len = buffer.len();

            // Check if cache is valid (same content length and width)
            let cache_valid = self.streaming_cache.is_some()
                && self.streaming_cache_len == buffer_len
                && self.streaming_cache_width == available_width;

            if !cache_valid {
                // Re-render and update cache
                let rendered = render_markdown_with_prefix(buffer, available_width, &theme);
                self.streaming_cache = Some(rendered);
                self.streaming_cache_len = buffer_len;
                self.streaming_cache_width = available_width;
            }

            // Use cached lines
            if let Some(ref cached) = self.streaming_cache {
                message_lines.extend(cached.iter().cloned());
            }

            // Add cursor on last line
            if let Some(last) = message_lines.last_mut() {
                last.spans
                    .push(Span::styled("\u{2588}", theme.cursor));
            }
        } else if let Some(status) = pending_status {
            // Show pending status with spinner when not streaming
            let spinner_char = self.config.spinner_chars.get(self.spinner_index).copied().unwrap_or(' ');
            message_lines.push(Line::from(vec![
                Span::styled(format!("{} ", spinner_char), theme.throbber_spinner),
                Span::styled(status, theme.throbber_label),
            ]));
        }

        // Calculate scroll (no wrapping estimation needed - we do manual wrapping)
        let available_height = area.height.saturating_sub(2) as usize; // -2 for border + padding
        let total_lines = message_lines.len();
        let max_scroll = total_lines.saturating_sub(available_height) as u16;
        self.last_max_scroll = max_scroll;

        // Scroll behavior:
        // - scroll_offset = u16::MAX: auto-scroll to bottom (show latest)
        // - other values: manual scroll position (clamped to valid range)
        //
        // Important: Update self.scroll_offset when clamping to prevent stale values
        // after terminal resize. This ensures scroll_up/scroll_down work correctly
        // immediately after resize.
        let scroll_offset = if self.scroll_offset == u16::MAX {
            max_scroll
        } else {
            let clamped = self.scroll_offset.min(max_scroll);
            if clamped != self.scroll_offset {
                self.scroll_offset = clamped;
            }
            clamped
        };

        let messages_widget = Paragraph::new(message_lines)
            .block(content_block)
            .style(theme.background.patch(theme.text))
            .scroll((scroll_offset, 0));
        frame.render_widget(messages_widget, area);
    }
}

/// Render a tool execution message
fn render_tool_message(
    data: &ToolMessageData,
    config: &ChatViewConfig,
    available_width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let theme = app_theme();

    // Line 1: tool icon + DisplayName(DisplayTitle)
    let header = if data.display_title.is_empty() {
        format!("{} {}", config.tool_icon, data.display_name)
    } else {
        format!("{} {}({})", config.tool_icon, data.display_name, data.display_title)
    };
    lines.push(Line::from(Span::styled(header, theme.tool_header)));

    // Line 2+: Status with appropriate icon and color
    match &data.status {
        ToolStatus::Executing => {
            lines.push(Line::from(Span::styled(
                format!("   {} executing...", config.tool_executing_arrow),
                theme.tool_executing,
            )));
        }
        ToolStatus::WaitingForUser => {
            lines.push(Line::from(Span::styled(
                format!("   {} waiting for user...", config.tool_executing_arrow),
                theme.tool_executing,
            )));
        }
        ToolStatus::Completed => {
            lines.push(Line::from(Span::styled(
                format!("   {} Completed", config.tool_completed_checkmark),
                theme.tool_completed,
            )));
        }
        ToolStatus::Failed(err) => {
            // Wrap long error messages with proper indentation
            let prefix = format!("   {} ", config.tool_failed_icon);
            let cont_prefix = "     "; // Align continuation with error text
            let wrapped = wrap_with_prefix(
                err,
                &prefix,
                theme.tool_failed,
                cont_prefix,
                available_width,
                &theme,
            );
            lines.extend(wrapped);
        }
    }

    lines
}

impl Default for ChatView {
    fn default() -> Self {
        Self::new()
    }
}

// --- ConversationView trait implementation ---

use super::ConversationView;

/// State snapshot for ChatView (used for session save/restore)
#[derive(Clone)]
struct ChatViewState {
    messages: Vec<MessageSnapshot>,
    scroll_offset: u16,
    streaming_buffer: Option<String>,
    last_max_scroll: u16,
    auto_scroll_enabled: bool,
    tool_index: HashMap<String, usize>,
    spinner_index: usize,
}

/// Snapshot of a single message (Clone-friendly version)
#[derive(Clone)]
struct MessageSnapshot {
    role: MessageRole,
    content: String,
    timestamp: DateTime<Local>,
    tool_data: Option<ToolMessageData>,
}

impl From<&Message> for MessageSnapshot {
    fn from(msg: &Message) -> Self {
        Self {
            role: msg.role,
            content: msg.content.clone(),
            timestamp: msg.timestamp,
            tool_data: msg.tool_data.clone(),
        }
    }
}

impl From<MessageSnapshot> for Message {
    fn from(snapshot: MessageSnapshot) -> Self {
        Self {
            role: snapshot.role,
            content: snapshot.content,
            timestamp: snapshot.timestamp,
            cached_lines: None,
            cached_width: 0,
            tool_data: snapshot.tool_data,
        }
    }
}

impl ConversationView for ChatView {
    fn add_user_message(&mut self, content: String) {
        ChatView::add_user_message(self, content);
    }

    fn add_assistant_message(&mut self, content: String) {
        ChatView::add_assistant_message(self, content);
    }

    fn add_system_message(&mut self, content: String) {
        ChatView::add_system_message(self, content);
    }

    fn append_streaming(&mut self, text: &str) {
        ChatView::append_streaming(self, text);
    }

    fn complete_streaming(&mut self) {
        ChatView::complete_streaming(self);
    }

    fn discard_streaming(&mut self) {
        ChatView::discard_streaming(self);
    }

    fn is_streaming(&self) -> bool {
        ChatView::is_streaming(self)
    }

    fn add_tool_message(&mut self, tool_use_id: &str, display_name: &str, display_title: &str) {
        ChatView::add_tool_message(self, tool_use_id, display_name, display_title);
    }

    fn update_tool_status(&mut self, tool_use_id: &str, status: ToolStatus) {
        ChatView::update_tool_status(self, tool_use_id, status);
    }

    fn scroll_up(&mut self) {
        ChatView::scroll_up(self);
    }

    fn scroll_down(&mut self) {
        ChatView::scroll_down(self);
    }

    fn enable_auto_scroll(&mut self) {
        ChatView::enable_auto_scroll(self);
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme, pending_status: Option<&str>) {
        self.render_chat(frame, area, pending_status);
    }

    fn step_spinner(&mut self) {
        ChatView::step_spinner(self);
    }

    fn save_state(&self) -> Box<dyn Any + Send> {
        let state = ChatViewState {
            messages: self.messages.iter().map(MessageSnapshot::from).collect(),
            scroll_offset: self.scroll_offset,
            streaming_buffer: self.streaming_buffer.clone(),
            last_max_scroll: self.last_max_scroll,
            auto_scroll_enabled: self.auto_scroll_enabled,
            tool_index: self.tool_index.clone(),
            spinner_index: self.spinner_index,
        };
        Box::new(state)
    }

    fn restore_state(&mut self, state: Box<dyn Any + Send>) {
        if let Ok(chat_state) = state.downcast::<ChatViewState>() {
            self.messages = chat_state.messages.into_iter().map(Message::from).collect();
            self.scroll_offset = chat_state.scroll_offset;
            self.streaming_buffer = chat_state.streaming_buffer;
            // Clear streaming cache (will be regenerated on next render)
            self.streaming_cache = None;
            self.streaming_cache_len = 0;
            self.streaming_cache_width = 0;
            self.last_max_scroll = chat_state.last_max_scroll;
            self.auto_scroll_enabled = chat_state.auto_scroll_enabled;
            self.tool_index = chat_state.tool_index;
            self.spinner_index = chat_state.spinner_index;
        }
    }

    fn clear(&mut self) {
        self.messages.clear();
        self.streaming_buffer = None;
        self.streaming_cache = None;
        self.streaming_cache_len = 0;
        self.streaming_cache_width = 0;
        self.tool_index.clear();
        self.scroll_offset = 0;
        self.last_max_scroll = 0;
        self.auto_scroll_enabled = true;
        self.spinner_index = 0;
        // Preserve: title, render_title, render_initial_content, config
    }
}

// --- Widget trait implementation ---

use std::any::Any;
use crossterm::event::KeyEvent;
use super::{widget_ids, Widget, WidgetKeyContext, WidgetKeyResult};

impl Widget for ChatView {
    fn id(&self) -> &'static str {
        widget_ids::CHAT_VIEW
    }

    fn priority(&self) -> u8 {
        50 // Low priority - core widget, handles scroll events
    }

    fn is_active(&self) -> bool {
        true // Always active
    }

    fn handle_key(&mut self, _key: KeyEvent, _ctx: &WidgetKeyContext) -> WidgetKeyResult {
        // ChatView doesn't handle keys directly via Widget trait
        // Scrolling is handled by App
        WidgetKeyResult::NotHandled
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        // Note: This is a simplified render without pending_status
        // App may use render_chat method with pending_status directly for richer status
        self.render_chat(frame, area, None);
    }

    fn required_height(&self, _available: u16) -> u16 {
        0 // ChatView takes remaining space via layout
    }

    fn blocks_input(&self) -> bool {
        false
    }

    fn is_overlay(&self) -> bool {
        false
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

