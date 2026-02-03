//! Text input buffer with cursor management
//!
//! A simple text input widget that handles cursor movement, editing operations,
//! and word wrapping calculations for terminal display.

/// Text input buffer with cursor management
pub struct TextInput {
    buffer: String,
    cursor_pos: usize,
}

impl TextInput {
    /// Create a new empty text input
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor_pos: 0,
        }
    }

    /// Get the current buffer contents
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Take ownership of the buffer contents, leaving it empty
    pub fn take(&mut self) -> String {
        self.cursor_pos = 0;
        std::mem::take(&mut self.buffer)
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor_pos = 0;
    }

    // Editing operations

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    /// Delete the character before the cursor (backspace)
    pub fn delete_char_before(&mut self) {
        if self.cursor_pos > 0 {
            let prev_char_boundary = self.buffer[..self.cursor_pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.buffer.remove(prev_char_boundary);
            self.cursor_pos = prev_char_boundary;
        }
    }

    /// Delete the character at the cursor position (delete key)
    pub fn delete_char_at(&mut self) {
        if self.cursor_pos < self.buffer.len() {
            self.buffer.remove(self.cursor_pos);
        }
    }

    /// Kill (delete) from cursor to end of line (Ctrl-K)
    pub fn kill_line(&mut self) {
        let line_end = self.buffer[self.cursor_pos..]
            .find('\n')
            .map(|i| self.cursor_pos + i)
            .unwrap_or(self.buffer.len());

        if line_end == self.cursor_pos && self.cursor_pos < self.buffer.len() {
            // At end of line, delete the newline
            self.buffer.remove(self.cursor_pos);
        } else {
            // Delete from cursor to end of line
            self.buffer.drain(self.cursor_pos..line_end);
        }
    }

    // Cursor movement operations

    /// Move cursor left one character
    pub fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos = self.buffer[..self.cursor_pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right one character
    pub fn move_right(&mut self) {
        if self.cursor_pos < self.buffer.len() {
            self.cursor_pos += self.buffer[self.cursor_pos..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
        }
    }

    /// Move cursor up one line
    pub fn move_up(&mut self) {
        let (current_line, col) = self.cursor_line_col();
        if current_line > 0 {
            let lines: Vec<&str> = self.buffer.split('\n').collect();
            let prev_line_len = lines[current_line - 1].len();
            let new_col = col.min(prev_line_len);
            self.cursor_pos = self.line_col_to_pos(current_line - 1, new_col);
        }
    }

    /// Move cursor down one line
    pub fn move_down(&mut self) {
        let (current_line, col) = self.cursor_line_col();
        let lines: Vec<&str> = self.buffer.split('\n').collect();
        if current_line < lines.len() - 1 {
            let next_line_len = lines[current_line + 1].len();
            let new_col = col.min(next_line_len);
            self.cursor_pos = self.line_col_to_pos(current_line + 1, new_col);
        }
    }

    /// Move cursor to the start of the current line
    pub fn move_to_line_start(&mut self) {
        let (current_line, _) = self.cursor_line_col();
        self.cursor_pos = self.line_col_to_pos(current_line, 0);
    }

    /// Move cursor to the end of the current line
    pub fn move_to_line_end(&mut self) {
        let (current_line, _) = self.cursor_line_col();
        let lines: Vec<&str> = self.buffer.split('\n').collect();
        let line_len = lines.get(current_line).map(|l| l.len()).unwrap_or(0);
        self.cursor_pos = self.line_col_to_pos(current_line, line_len);
    }

    // Utility methods

    /// Get the current cursor position as (line, column)
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let before_cursor = &self.buffer[..self.cursor_pos];
        let line = before_cursor.matches('\n').count();
        let col = before_cursor
            .rfind('\n')
            .map(|i| self.cursor_pos - i - 1)
            .unwrap_or(self.cursor_pos);
        (line, col)
    }

    fn line_col_to_pos(&self, line: usize, col: usize) -> usize {
        let mut pos = 0;
        for (i, l) in self.buffer.split('\n').enumerate() {
            if i == line {
                return pos + col.min(l.len());
            }
            pos += l.len() + 1; // +1 for newline
        }
        self.buffer.len()
    }

    /// Get the number of logical lines in the buffer
    pub fn line_count(&self) -> usize {
        self.buffer.split('\n').count().max(1)
    }

    /// Simulate word wrapping for a line and return visual line break positions
    /// Returns a vec of (start_char_idx, end_char_idx) for each visual line
    fn word_wrap_line(
        &self,
        line: &str,
        first_line_width: usize,
        subsequent_width: usize,
    ) -> Vec<(usize, usize)> {
        let chars: Vec<char> = line.chars().collect();
        let mut breaks = Vec::new();

        if chars.is_empty() {
            breaks.push((0, 0));
            return breaks;
        }

        let mut start = 0;
        let mut is_first_line = true;

        while start < chars.len() {
            let width = if is_first_line {
                first_line_width
            } else {
                subsequent_width
            };
            let max_end = (start + width).min(chars.len());

            if max_end >= chars.len() {
                // Rest of line fits
                breaks.push((start, chars.len()));
                break;
            }

            // Look for last space within width to break at (word wrap)
            // If no space found, break_at stays at max_end (character wrap)
            let mut break_at = max_end;
            for i in (start..max_end).rev() {
                if chars[i] == ' ' {
                    break_at = i + 1; // Break after the space
                    break;
                }
            }

            breaks.push((start, break_at));
            start = break_at;
            is_first_line = false;
        }

        if breaks.is_empty() {
            breaks.push((0, 0));
        }

        breaks
    }

    /// Calculate visual line count accounting for word wrapping
    pub fn visual_line_count(&self, width: usize, prompt_len: usize, indent_len: usize) -> usize {
        if width == 0 {
            return self.line_count();
        }

        let mut visual_lines = 0;
        for (i, line) in self.buffer.split('\n').enumerate() {
            let prefix_len = if i == 0 { prompt_len } else { indent_len };
            let first_width = width.saturating_sub(prefix_len);
            let breaks = self.word_wrap_line(line, first_width, width);
            visual_lines += breaks.len();
        }
        visual_lines.max(1)
    }

    /// Calculate cursor display position accounting for word wrapping
    pub fn cursor_display_position_wrapped(
        &self,
        width: usize,
        prompt_len: usize,
        indent_len: usize,
    ) -> (u16, u16) {
        if width == 0 {
            return self.cursor_display_position(prompt_len, indent_len);
        }

        let (logical_line, col) = self.cursor_line_col();

        // Convert byte-based col to character count
        let current_line = self.buffer.split('\n').nth(logical_line).unwrap_or("");
        let col_chars = current_line[..col.min(current_line.len())]
            .chars()
            .count();

        // Count visual lines before cursor's logical line
        let mut visual_y = 0;
        for (i, line) in self.buffer.split('\n').enumerate() {
            if i >= logical_line {
                break;
            }
            let prefix_len = if i == 0 { prompt_len } else { indent_len };
            let first_width = width.saturating_sub(prefix_len);
            let breaks = self.word_wrap_line(line, first_width, width);
            visual_y += breaks.len();
        }

        // Find cursor position within current logical line
        let prefix_len = if logical_line == 0 {
            prompt_len
        } else {
            indent_len
        };
        let first_width = width.saturating_sub(prefix_len);
        let breaks = self.word_wrap_line(current_line, first_width, width);

        // Find which visual line contains the cursor.
        // Breaks are (start, end) where start is inclusive and end is exclusive.
        // A cursor at position N belongs to the segment where start <= N < end.
        // Cursor at exactly 'end' belongs to the NEXT segment (or fallback if last).
        for (i, (start, end)) in breaks.iter().enumerate() {
            if col_chars >= *start && col_chars < *end {
                let x = if i == 0 {
                    prefix_len + (col_chars - start)
                } else {
                    col_chars - start
                };
                return (x as u16, (visual_y + i) as u16);
            }
        }

        // Fallback: cursor at end of last visual line (col_chars == last segment's end)
        let last_break = breaks.last().unwrap_or(&(0, 0));
        let x = if breaks.len() == 1 {
            prefix_len + (col_chars - last_break.0)
        } else {
            col_chars - last_break.0
        };
        (x as u16, (visual_y + breaks.len() - 1) as u16)
    }

    /// Calculate cursor display position without word wrapping
    pub fn cursor_display_position(&self, prompt_len: usize, indent_len: usize) -> (u16, u16) {
        let (line, col) = self.cursor_line_col();
        let x = if line == 0 {
            prompt_len + col
        } else {
            indent_len + col
        };
        (x as u16, line as u16)
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

// --- Widget trait implementation ---

use std::any::Any;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
use crate::themes::Theme;
use super::{widget_ids, Widget, WidgetKeyContext, WidgetKeyResult};

impl Widget for TextInput {
    fn id(&self) -> &'static str {
        widget_ids::TEXT_INPUT
    }

    fn priority(&self) -> u8 {
        50 // Low priority - core widget
    }

    fn is_active(&self) -> bool {
        true // Always active (unless blocked by modal)
    }

    fn handle_key(&mut self, _key: KeyEvent, _ctx: &WidgetKeyContext) -> WidgetKeyResult {
        // TextInput doesn't handle keys via Widget trait
        // Key handling is done by App directly
        WidgetKeyResult::NotHandled
    }

    fn render(&mut self, _frame: &mut Frame, _area: Rect, _theme: &Theme) {
        // TextInput rendering is handled by App directly
        // This is a no-op for Widget trait compatibility
    }

    fn required_height(&self, _available: u16) -> u16 {
        0 // Height calculated by App based on content
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_input() {
        let input = TextInput::new();
        assert!(input.is_empty());
        assert_eq!(input.buffer(), "");
    }

    #[test]
    fn test_insert_char() {
        let mut input = TextInput::new();
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');
        assert_eq!(input.buffer(), "abc");
    }

    #[test]
    fn test_delete_char_before() {
        let mut input = TextInput::new();
        input.insert_char('a');
        input.insert_char('b');
        input.delete_char_before();
        assert_eq!(input.buffer(), "a");
    }

    #[test]
    fn test_cursor_movement() {
        let mut input = TextInput::new();
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');

        input.move_left();
        input.move_left();
        input.insert_char('x');
        assert_eq!(input.buffer(), "axbc");
    }

    #[test]
    fn test_cursor_line_col() {
        let mut input = TextInput::new();
        input.insert_char('a');
        input.insert_char('\n');
        input.insert_char('b');

        let (line, col) = input.cursor_line_col();
        assert_eq!(line, 1);
        assert_eq!(col, 1);
    }

    #[test]
    fn test_line_count() {
        let mut input = TextInput::new();
        assert_eq!(input.line_count(), 1);

        input.insert_char('a');
        input.insert_char('\n');
        input.insert_char('b');
        assert_eq!(input.line_count(), 2);

        input.insert_char('\n');
        input.insert_char('c');
        assert_eq!(input.line_count(), 3);
    }

    #[test]
    fn test_take() {
        let mut input = TextInput::new();
        input.insert_char('a');
        input.insert_char('b');

        let taken = input.take();
        assert_eq!(taken, "ab");
        assert!(input.is_empty());
    }
}
