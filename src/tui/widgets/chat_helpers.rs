//! Helper functions for ChatView rendering customization
//!
//! These helpers make it easy to create common empty state and header renderers
//! for ChatView widgets without writing raw ratatui code.
//!
//! # Examples
//!
//! ```rust,ignore
//! use agent_core::tui::widgets::{ChatView, welcome_art, centered_text};
//!
//! // Simple centered text
//! let chat = ChatView::new()
//!     .with_empty_state(centered_text("Welcome! Type a message to begin."));
//!
//! // ASCII art welcome screen
//! let chat = ChatView::new()
//!     .with_empty_state(welcome_art(&[
//!         "  __  __       _ _   _  ____          _      ",
//!         " |  \\/  |_   _| | |_(_)/ ___|___   __| | ___ ",
//!         "",
//!         "    Type a message to start...",
//!     ]));
//! ```

use ratatui::{
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::themes::Theme;

/// Type alias for render callbacks used by ChatView
///
/// Render functions receive the frame, area to render in, and current theme.
pub type RenderFn = Box<dyn Fn(&mut Frame, Rect, &Theme) + Send + Sync>;

/// Create an ASCII art welcome screen renderer
///
/// Renders the provided lines centered vertically in the available area.
/// Each line can have different styling based on the `subtitle_indices` parameter.
///
/// # Arguments
/// * `lines` - The ASCII art lines to display
/// * `subtitle_indices` - Indices of lines that should use subtitle styling (dimmer)
///
/// # Example
/// ```rust,ignore
/// let renderer = welcome_art_styled(
///     &[
///         "  _____         _   ",
///         " |_   _|__  ___| |_ ",
///         "   | |/ _ \\/ __| __|",
///         "",
///         "  Welcome to the app",
///     ],
///     &[4], // Line 4 is a subtitle
/// );
/// ```
pub fn welcome_art_styled(
    lines: &[&str],
    subtitle_indices: &[usize],
) -> RenderFn {
    let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    let subtitle_indices: Vec<usize> = subtitle_indices.to_vec();

    Box::new(move |frame: &mut Frame, area: Rect, theme: &Theme| {
        let total_lines = lines.len() as u16;
        let start_y = area.y + area.height.saturating_sub(total_lines) / 2;

        for (i, line) in lines.iter().enumerate() {
            let y = start_y + i as u16;
            if y >= area.y + area.height {
                break;
            }

            // Use heading_1 style for main text (typically cyan/prominent)
            // Use timestamp style for subtitles (typically dim)
            let style = if subtitle_indices.contains(&i) {
                theme.timestamp
            } else {
                theme.heading_1
            };

            let paragraph = Paragraph::new(line.as_str()).style(style);
            frame.render_widget(
                paragraph,
                Rect::new(area.x, y, area.width, 1),
            );
        }
    })
}

/// Create a simple ASCII art welcome screen renderer
///
/// All lines use the same welcome text style.
/// For styled subtitles, use `welcome_art_styled` instead.
///
/// # Example
/// ```rust,ignore
/// let chat = ChatView::new()
///     .with_empty_state(welcome_art(&[
///         "  __  __       _ _   _  ____          _      ",
///         " |  \\/  |_   _| | |_(_)/ ___|___   __| | ___ ",
///         "",
///         "    Type a message to start...",
///     ]));
/// ```
pub fn welcome_art(lines: &[&str]) -> RenderFn {
    welcome_art_styled(lines, &[])
}

/// Create a centered text message renderer
///
/// Renders a single line of text centered both horizontally and vertically.
///
/// # Example
/// ```rust,ignore
/// let chat = ChatView::new()
///     .with_empty_state(centered_text("Welcome! Type a message to begin."));
/// ```
pub fn centered_text(text: &str) -> RenderFn {
    let text = text.to_string();

    Box::new(move |frame: &mut Frame, area: Rect, theme: &Theme| {
        let paragraph = Paragraph::new(text.as_str())
            .style(theme.text)
            .alignment(Alignment::Center);

        let y = area.y + area.height / 2;
        frame.render_widget(
            paragraph,
            Rect::new(area.x, y, area.width, 1),
        );
    })
}

/// Create a title bar renderer with left and right content
///
/// Renders a single-line title bar with left-aligned and right-aligned text.
///
/// # Example
/// ```rust,ignore
/// let chat = ChatView::new()
///     .with_header(title_bar("Agent Name", "Status: Ready"));
/// ```
pub fn title_bar(left: &str, right: &str) -> RenderFn {
    let left = left.to_string();
    let right = right.to_string();

    Box::new(move |frame: &mut Frame, area: Rect, theme: &Theme| {
        let width = area.width as usize;
        let left_len = left.chars().count();
        let right_len = right.chars().count();

        // Calculate padding between left and right
        let padding = width.saturating_sub(left_len + right_len);

        let line = Line::from(vec![
            Span::styled(left.as_str(), theme.title_text),
            Span::raw(" ".repeat(padding)),
            Span::styled(right.as_str(), theme.title_text),
        ]);

        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
    })
}

/// Create a multi-line centered content renderer
///
/// Renders multiple lines centered vertically in the available area.
/// Each line is rendered with the default text style.
///
/// # Example
/// ```rust,ignore
/// let chat = ChatView::new()
///     .with_empty_state(centered_lines(&[
///         "Welcome to the application",
///         "",
///         "Type a message to get started",
///     ]));
/// ```
pub fn centered_lines(lines: &[&str]) -> RenderFn {
    let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();

    Box::new(move |frame: &mut Frame, area: Rect, theme: &Theme| {
        let total_lines = lines.len() as u16;
        let start_y = area.y + area.height.saturating_sub(total_lines) / 2;

        for (i, line) in lines.iter().enumerate() {
            let y = start_y + i as u16;
            if y >= area.y + area.height {
                break;
            }

            let paragraph = Paragraph::new(line.as_str())
                .style(theme.text)
                .alignment(Alignment::Center);

            frame.render_widget(
                paragraph,
                Rect::new(area.x, y, area.width, 1),
            );
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_welcome_art_creates_render_fn() {
        let _render_fn = welcome_art(&["Line 1", "Line 2"]);
        // Just verify it compiles and creates a RenderFn
    }

    #[test]
    fn test_centered_text_creates_render_fn() {
        let _render_fn = centered_text("Hello");
        // Just verify it compiles and creates a RenderFn
    }

    #[test]
    fn test_title_bar_creates_render_fn() {
        let _render_fn = title_bar("Left", "Right");
        // Just verify it compiles and creates a RenderFn
    }
}
