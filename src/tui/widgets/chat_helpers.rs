//! Helper types for ChatView rendering customization

use ratatui::{layout::Rect, Frame};

use crate::tui::themes::Theme;

/// Type alias for render callbacks used by ChatView
///
/// Render functions receive the frame, area to render in, and current theme.
/// These are used for custom initial content and header rendering.
///
/// # Example
///
/// ```rust,ignore
/// use agent_core::tui::widgets::RenderFn;
///
/// let render_fn: RenderFn = Box::new(|frame, area, theme| {
///     // Custom rendering logic here
/// });
/// ```
pub type RenderFn = Box<dyn Fn(&mut Frame, Rect, &Theme) + Send + Sync>;
