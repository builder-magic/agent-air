//! Core types for key handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Result of handling a key event at the App level.
#[derive(Debug, Clone, PartialEq)]
pub enum AppKeyResult {
    /// Key was handled, stop processing.
    Handled,
    /// Key was not handled, continue to widget dispatch then default handling.
    NotHandled,
    /// Execute an application action.
    Action(AppKeyAction),
}

/// Application-level key actions.
///
/// These are commands that the App knows how to execute. The KeyHandler
/// doesn't call methods directly - it returns an Action and the App executes it.
/// This keeps the handler decoupled from App internals.
#[derive(Debug, Clone, PartialEq)]
pub enum AppKeyAction {
    // Navigation (for text input)
    /// Move cursor up one line.
    MoveUp,
    /// Move cursor down one line.
    MoveDown,
    /// Move cursor left one character.
    MoveLeft,
    /// Move cursor right one character.
    MoveRight,
    /// Move cursor to the start of the current line.
    MoveLineStart,
    /// Move cursor to the end of the current line.
    MoveLineEnd,

    // Editing
    /// Delete character before cursor (Backspace).
    DeleteCharBefore,
    /// Delete character at cursor (Delete).
    DeleteCharAt,
    /// Kill text from cursor to end of line.
    KillLine,
    /// Insert a newline character in multi-line input.
    InsertNewline,
    /// Insert a character.
    InsertChar(char),

    // Application control
    /// Submit the current message.
    Submit,
    /// Interrupt the current LLM request.
    Interrupt,
    /// Quit the application immediately (force quit, Ctrl+Q).
    Quit,
    /// Request exit - handler manages confirmation, App calls ExitHandler.
    RequestExit,

    // Widget activation
    /// Activate the slash command popup.
    ActivateSlashPopup,
}

/// Context provided to the KeyHandler.
///
/// This struct provides information about the current application state
/// so the handler can make decisions based on context.
#[derive(Debug, Clone)]
pub struct KeyContext {
    /// Whether the input buffer is empty.
    pub input_empty: bool,
    /// Whether currently processing (waiting for LLM/tools).
    pub is_processing: bool,
    /// Whether a modal widget is blocking input (e.g., question panel).
    pub widget_blocking: bool,
}

/// A key combination (key + modifiers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    /// The key code.
    pub code: KeyCode,
    /// The modifier keys (Ctrl, Shift, Alt).
    pub modifiers: KeyModifiers,
}

impl KeyCombo {
    /// Create a new key combination.
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    /// Create a key combination with no modifiers.
    pub const fn key(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// Create a Ctrl+key combination.
    pub const fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
        }
    }

    /// Check if this combo matches a key event.
    pub fn matches(&self, event: &KeyEvent) -> bool {
        self.code == event.code && self.modifiers == event.modifiers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_combo_matches() {
        let combo = KeyCombo::ctrl('d');
        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert!(combo.matches(&event));

        let event2 = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
        assert!(!combo.matches(&event2));
    }

    #[test]
    fn test_key_combo_key() {
        let combo = KeyCombo::key(KeyCode::Enter);
        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert!(combo.matches(&event));
    }
}
