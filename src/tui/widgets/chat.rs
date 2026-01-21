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

use crate::tui::themes::theme as app_theme;
use crate::tui::markdown::{render_markdown_with_prefix, wrap_with_prefix};

// First line prefixes (symbol + space)
const USER_PREFIX: &str = "> ";
const SYSTEM_PREFIX: &str = "* ";
const TIMESTAMP_PREFIX: &str = "  - ";
// Continuation line prefix (spaces to align with text after symbol)
const CONTINUATION: &str = "  ";
// Spinner characters for pending status animation
const SPINNER_CHARS: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

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
#[derive(Debug, Clone)]
pub struct ToolMessageData {
    #[allow(dead_code)] // Used as HashMap key, kept here for debugging
    pub tool_use_id: String,
    pub display_name: String,
    pub display_title: String,
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
    fn get_rendered_lines(&mut self, available_width: usize) -> &[Line<'static>] {
        // Invalidate cache if width changed
        if self.cached_width != available_width {
            self.cached_lines = None;
        }

        // Render and cache if needed
        if self.cached_lines.is_none() {
            let lines = self.render_lines(available_width);
            self.cached_lines = Some(lines);
            self.cached_width = available_width;
        }

        self.cached_lines.as_ref().unwrap()
    }

    /// Render this message to lines (called only when cache is invalid)
    fn render_lines(&self, available_width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let t = app_theme();

        match self.role {
            MessageRole::User => {
                let rendered = wrap_with_prefix(
                    &self.content,
                    USER_PREFIX,
                    t.user_prefix,
                    CONTINUATION,
                    available_width,
                    &t,
                );
                lines.extend(rendered);
            }
            MessageRole::System => {
                let rendered = wrap_with_prefix(
                    &self.content,
                    SYSTEM_PREFIX,
                    t.system_prefix,
                    CONTINUATION,
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
                    lines.extend(render_tool_message(data));
                }
            }
        }

        // Timestamp line (only for user and system messages)
        // Note: Using %I (with leading zero) instead of %-I for cross-platform compatibility
        if self.role != MessageRole::Assistant && self.role != MessageRole::Tool {
            let time_str = self.timestamp.format("%I:%M:%S %p").to_string();
            let timestamp_text = format!("{}{}", TIMESTAMP_PREFIX, time_str);
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

pub struct ChatView {
    messages: Vec<Message>,
    scroll_offset: u16,
    /// Buffer for streaming assistant response
    streaming_buffer: Option<String>,
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
    /// Optional custom empty state renderer (shown when no messages)
    render_empty_state: Option<RenderFn>,
}

impl ChatView {
    /// Create a new ChatView with default settings
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            streaming_buffer: None,
            last_max_scroll: 0,
            auto_scroll_enabled: true,
            tool_index: HashMap::new(),
            spinner_index: 0,
            title: "Chat".to_string(),
            render_empty_state: None,
        }
    }

    /// Set the title displayed in the title bar
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set custom empty state renderer (shown when no messages)
    ///
    /// Use helper functions from `chat_helpers` for common patterns:
    /// - `welcome_art()` - ASCII art welcome screen
    /// - `centered_text()` - Simple centered message
    ///
    /// Or provide a custom closure for full ratatui control.
    pub fn with_empty_state(mut self, render: RenderFn) -> Self {
        self.render_empty_state = Some(render);
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
        self.spinner_index = (self.spinner_index + 1) % SPINNER_CHARS.len();
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
    }

    /// Discard the streaming buffer without saving (used on cancel)
    pub fn discard_streaming(&mut self) {
        self.streaming_buffer = None;
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
        // Green checkmark style for agent status
        let check_style = Style::default().fg(Color::Green);

        // Helper to create title lines (called multiple times if needed)
        let create_titles = || {
            let left = Line::from(vec![
                Span::styled("\u{2500} ", app_theme().title_separator),
                Span::styled("\u{25CF} ", app_theme().title_indicator_connected),
                Span::styled(self.title.clone(), app_theme().title_text),
            ]);

            let right = Line::from(vec![
                Span::styled("[\u{2713}]", check_style),
                Span::styled(" Manager Agent (1) ", app_theme().title_text),
                Span::styled("[\u{2713}]", check_style),
                Span::styled(" Coding Agents (4) ", app_theme().title_text),
                Span::styled("[\u{2713}]", check_style),
                Span::styled(" Code Reviewers (2) ", app_theme().title_text),
                Span::styled("\u{2500}", app_theme().title_separator),
            ]);

            (left, right)
        };

        // Check if we're in empty state with a custom renderer
        let is_empty_state = self.messages.is_empty() && self.streaming_buffer.is_none() && pending_status.is_none();

        // If we have a custom empty state renderer, use it and return early
        if is_empty_state {
            if let Some(ref render_fn) = self.render_empty_state {
                let (left_title, right_title) = create_titles();

                let empty_block = Block::default()
                    .title(left_title)
                    .title_alignment(Alignment::Left)
                    .title(right_title.alignment(Alignment::Right))
                    .borders(Borders::TOP)
                    .border_style(app_theme().border)
                    .padding(Padding::new(1, 0, 1, 0));

                let inner = empty_block.inner(area);
                frame.render_widget(empty_block, area);

                // Call the custom renderer
                render_fn(frame, inner, &app_theme());
                return;
            }
        }

        // Normal rendering path
        let (left_title, right_title) = create_titles();

        let content_block = Block::default()
            .title(left_title)
            .title_alignment(Alignment::Left)
            .title(right_title.alignment(Alignment::Right))
            .borders(Borders::TOP)
            .border_style(app_theme().border)
            .padding(Padding::new(1, 0, 1, 0)); // left=1, top=1

        // Calculate available width for manual wrapping
        let available_width = area.width.saturating_sub(2) as usize; // -2 for left padding + margin

        // Build message lines using cached rendering
        let mut message_lines: Vec<Line> = Vec::new();

        // Show default empty state if no custom renderer
        if is_empty_state {
            message_lines.push(Line::from(""));
            message_lines.push(Line::from(Span::styled(
                "    Type a message to start chatting...",
                Style::default().fg(Color::DarkGray),
            )));
        }

        for msg in &mut self.messages {
            // Use cached lines (renders only if cache is invalid)
            let cached = msg.get_rendered_lines(available_width);
            message_lines.extend(cached.iter().cloned());
        }

        // Add streaming buffer if present
        if let Some(ref buffer) = self.streaming_buffer {
            let rendered = render_markdown_with_prefix(buffer, available_width, &app_theme());
            message_lines.extend(rendered);
            // Add cursor on last line
            if let Some(last) = message_lines.last_mut() {
                last.spans
                    .push(Span::styled("\u{2588}", app_theme().cursor));
            }
        } else if let Some(status) = pending_status {
            // Show pending status with spinner when not streaming
            let spinner_char = SPINNER_CHARS[self.spinner_index];
            message_lines.push(Line::from(vec![
                Span::styled(format!("{} ", spinner_char), app_theme().throbber_spinner),
                Span::styled(status, app_theme().throbber_label),
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
            .style(app_theme().background.patch(app_theme().text))
            .scroll((scroll_offset, 0));
        frame.render_widget(messages_widget, area);
    }
}

/// Render a tool execution message
fn render_tool_message(data: &ToolMessageData) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Line 1: hammer and pick icon + DisplayName(DisplayTitle)
    let header = if data.display_title.is_empty() {
        format!("\u{2692} {}", data.display_name)
    } else {
        format!("\u{2692} {}({})", data.display_name, data.display_title)
    };
    lines.push(Line::from(Span::styled(header, app_theme().tool_header)));

    // Line 2: Status with appropriate icon and color
    let status_line = match &data.status {
        ToolStatus::Executing => Line::from(Span::styled(
            "   \u{2192} executing...".to_string(),
            app_theme().tool_executing,
        )),
        ToolStatus::WaitingForUser => Line::from(Span::styled(
            "   \u{2192} waiting for user...".to_string(),
            app_theme().tool_executing,
        )),
        ToolStatus::Completed => Line::from(Span::styled(
            "   \u{2713} Completed".to_string(),
            app_theme().tool_completed,
        )),
        ToolStatus::Failed(err) => Line::from(Span::styled(
            format!("   \u{26A0} {}", err),
            app_theme().tool_failed,
        )),
    };
    lines.push(status_line);

    lines
}

impl Default for ChatView {
    fn default() -> Self {
        Self::new()
    }
}

// --- Widget trait implementation ---

use std::any::Any;
use crossterm::event::KeyEvent;
use crate::tui::themes::Theme;
use super::{widget_ids, Widget, WidgetKeyResult};

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

    fn handle_key(&mut self, _key: KeyEvent, _theme: &Theme) -> WidgetKeyResult {
        // ChatView doesn't handle keys directly via Widget trait
        // Scrolling is handled by App
        WidgetKeyResult::NotHandled
    }

    fn render(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        // Note: This is a simplified render without pending_status
        // App should use render_chat method with pending_status directly
        let mut chat_view = self.clone_for_render();
        chat_view.render_chat(frame, area, None);
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

impl ChatView {
    /// Create a shallow clone for rendering (avoids borrow issues in Widget::render)
    fn clone_for_render(&self) -> Self {
        Self {
            messages: Vec::new(), // Empty - we won't modify during render
            scroll_offset: self.scroll_offset,
            streaming_buffer: self.streaming_buffer.clone(),
            last_max_scroll: self.last_max_scroll,
            auto_scroll_enabled: self.auto_scroll_enabled,
            tool_index: HashMap::new(),
            spinner_index: self.spinner_index,
            title: self.title.clone(),
            render_empty_state: None, // Not cloned - callbacks aren't Clone
        }
    }
}
