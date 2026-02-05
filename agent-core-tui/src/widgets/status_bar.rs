//! Status bar widget for displaying application state
//!
//! The status bar is a non-interactive widget that displays:
//! - Current working directory
//! - Model name
//! - Context usage (if applicable)
//! - Contextual help hints
//!
//! # Customization
//!
//! The status bar can be customized in several ways:
//!
//! 1. **Config flags** - Show/hide specific elements via [`StatusBarConfig`]
//! 2. **Custom renderer** - Complete control over display via [`StatusBarRenderer`]
//! 3. **Opt-out** - Unregister the widget or exclude from layout
//!
//! # Examples
//!
//! ```rust,ignore
//! // Default status bar (automatically registered)
//! let app = App::with_config(config);
//!
//! // Custom renderer
//! let status_bar = StatusBar::new()
//!     .with_renderer(|data, theme| {
//!         vec![Line::from(format!(" {} | {}", data.model_name, data.session_id))]
//!     });
//! app.register_widget(status_bar);
//!
//! // Hide specific elements
//! let status_bar = StatusBar::new()
//!     .with_config(StatusBarConfig {
//!         show_cwd: false,
//!         ..Default::default()
//!     });
//! ```

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::any::Any;
use std::time::Duration;

use super::{Widget, WidgetKeyContext, WidgetKeyResult};
use crate::themes::Theme;

/// Custom renderer function type for status bar content
pub type StatusBarRenderer = Box<dyn Fn(&StatusBarData, &Theme) -> Vec<Line<'static>> + Send>;

/// Configuration for the status bar widget
#[derive(Default)]
pub struct StatusBarConfig {
    /// Height of the status bar (default: 2)
    pub height: u16,
    /// Show current working directory (default: true)
    pub show_cwd: bool,
    /// Show model name (default: true)
    pub show_model: bool,
    /// Show context usage (default: true)
    pub show_context: bool,
    /// Show help hints (default: true)
    pub show_hints: bool,
    /// Custom content renderer (overrides all flags)
    pub content_renderer: Option<StatusBarRenderer>,
    /// Custom hint for unconfigured/no session state
    pub hint_unconfigured: Option<String>,
    /// Custom hint when input is empty and ready
    pub hint_ready: Option<String>,
    /// Custom hint when user is typing
    pub hint_typing: Option<String>,
}

impl StatusBarConfig {
    /// Create default configuration
    pub fn new() -> Self {
        Self {
            height: 2,
            show_cwd: true,
            show_model: true,
            show_context: true,
            show_hints: true,
            content_renderer: None,
            hint_unconfigured: None,
            hint_ready: None,
            hint_typing: None,
        }
    }
}

/// Data required for rendering the status bar
///
/// This struct is updated before each render by the App via [`StatusBar::update_data`].
#[derive(Clone, Default)]
pub struct StatusBarData {
    /// Current working directory
    pub cwd: String,
    /// Model name
    pub model_name: String,
    /// Context tokens used
    pub context_used: i64,
    /// Context token limit
    pub context_limit: i32,
    /// Current session ID
    pub session_id: i64,
    /// Status hint from key handler
    pub status_hint: Option<String>,
    /// Whether the app is waiting for a response
    pub is_waiting: bool,
    /// Time elapsed since waiting started
    pub waiting_elapsed: Option<Duration>,
    /// Whether the input is empty
    pub input_empty: bool,
    /// Whether panels are active (suppress hints)
    pub panels_active: bool,
}

/// Status bar widget implementation
pub struct StatusBar {
    active: bool,
    config: StatusBarConfig,
    pub(crate) data: StatusBarData,
}

impl StatusBar {
    /// Create a new status bar with default configuration
    pub fn new() -> Self {
        Self {
            active: true,
            config: StatusBarConfig::new(),
            data: StatusBarData::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: StatusBarConfig) -> Self {
        Self {
            active: true,
            config,
            data: StatusBarData::default(),
        }
    }

    /// Set a custom renderer function
    pub fn with_renderer<F>(mut self, renderer: F) -> Self
    where
        F: Fn(&StatusBarData, &Theme) -> Vec<Line<'static>> + Send + 'static,
    {
        self.config.content_renderer = Some(Box::new(renderer));
        self
    }

    /// Set the hint shown when no session/API key is configured
    ///
    /// Default: " No session - type /new-session to start"
    pub fn with_hint_unconfigured(mut self, hint: impl Into<String>) -> Self {
        self.config.hint_unconfigured = Some(hint.into());
        self
    }

    /// Set the hint shown when input is empty and ready
    ///
    /// Default: " Ctrl-D to exit"
    pub fn with_hint_ready(mut self, hint: impl Into<String>) -> Self {
        self.config.hint_ready = Some(hint.into());
        self
    }

    /// Set the hint shown when user is typing
    ///
    /// Default: " Shift-Enter to add a new line"
    pub fn with_hint_typing(mut self, hint: impl Into<String>) -> Self {
        self.config.hint_typing = Some(hint.into());
        self
    }

    /// Update the status bar data before rendering
    ///
    /// This should be called by App before layout computation.
    pub fn update_data(&mut self, data: StatusBarData) {
        self.data = data;
    }

    /// Render the default status bar content
    fn render_default(&self, theme: &Theme, width: usize) -> Vec<Line<'static>> {
        let data = &self.data;
        let config = &self.config;

        // Line 1: CWD + spacing + context + model
        let cwd_display = if config.show_cwd && !data.cwd.is_empty() {
            format!(" {}", data.cwd)
        } else {
            String::new()
        };

        let context_str = if config.show_context {
            Self::format_context_display(data)
        } else {
            String::new()
        };

        let context_style = Self::context_style(data, theme);

        let model_display = if config.show_model {
            format!("{} ", data.model_name)
        } else {
            String::new()
        };

        let cwd_len = cwd_display.chars().count();
        let context_len = context_str.chars().count();
        let model_len = model_display.chars().count();
        let spacing = if context_len > 0 { 2 } else { 0 };
        let total_right = context_len + spacing + model_len;
        let line1_padding = width.saturating_sub(cwd_len + total_right);

        let line1 = if context_len > 0 {
            Line::from(vec![
                Span::styled(cwd_display, theme.status_help),
                Span::raw(" ".repeat(line1_padding)),
                Span::styled(context_str, context_style),
                Span::raw("  "),
                Span::styled(model_display, theme.status_model),
            ])
        } else {
            Line::from(vec![
                Span::styled(cwd_display, theme.status_help),
                Span::raw(" ".repeat(line1_padding)),
                Span::styled(model_display, theme.status_model),
            ])
        };

        // Line 2: Help text (hint line)
        let help_text = if !config.show_hints || data.panels_active {
            String::new()
        } else if let Some(hint) = &data.status_hint {
            format!(" {}", hint)
        } else if data.is_waiting {
            let elapsed_str = data
                .waiting_elapsed
                .map(format_elapsed)
                .unwrap_or_else(|| "0s".to_string());
            format!(" escape to interrupt ({})", elapsed_str)
        } else if data.session_id == 0 {
            config
                .hint_unconfigured
                .clone()
                .unwrap_or_else(|| " No session - type /new-session to start".to_string())
        } else if data.input_empty {
            config
                .hint_ready
                .clone()
                .unwrap_or_else(|| " esc to exit".to_string())
        } else {
            config
                .hint_typing
                .clone()
                .unwrap_or_else(|| " enter to send · shift-enter for new line".to_string())
        };

        let line2 = Line::from(vec![Span::styled(help_text, theme.status_help)]);

        vec![line1, line2]
    }

    /// Format the context display string
    fn format_context_display(data: &StatusBarData) -> String {
        if data.context_limit == 0 {
            return String::new();
        }

        let utilization = (data.context_used as f64 / data.context_limit as f64) * 100.0;
        let prefix = if utilization > 80.0 {
            "Context Low:"
        } else {
            "Context:"
        };

        format!(
            "{} {}/{} ({:.0}%)",
            prefix,
            format_tokens(data.context_used),
            format_tokens(data.context_limit as i64),
            utilization
        )
    }

    /// Get the style for context display based on utilization
    fn context_style(data: &StatusBarData, theme: &Theme) -> Style {
        if data.context_limit == 0 {
            return theme.status_help;
        }

        let utilization = (data.context_used as f64 / data.context_limit as f64) * 100.0;
        if utilization > 80.0 {
            Style::default().fg(Color::Yellow)
        } else {
            theme.status_help
        }
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for StatusBar {
    fn id(&self) -> &'static str {
        super::widget_ids::STATUS_BAR
    }

    fn priority(&self) -> u8 {
        100
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn handle_key(&mut self, _key: KeyEvent, _ctx: &WidgetKeyContext) -> WidgetKeyResult {
        // Status bar is non-interactive
        WidgetKeyResult::NotHandled
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let width = area.width as usize;

        let lines = if let Some(renderer) = &self.config.content_renderer {
            renderer(&self.data, theme)
        } else {
            self.render_default(theme, width)
        };

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn required_height(&self, _available: u16) -> u16 {
        self.config.height
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

/// Format a duration for display (e.g., "2s", "1m 30s", "2h 5m")
fn format_elapsed(duration: Duration) -> String {
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
