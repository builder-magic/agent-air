//! Core types for key handling.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

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
///
/// Note: This enum implements `Clone` and `PartialEq` manually due to the
/// `Custom` variant containing a trait object. Custom actions are compared
/// by Arc pointer equality and cloned by Arc reference counting.
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

    /// Custom action for user-defined behavior.
    ///
    /// The handler returns this, and the app receives it via a callback.
    /// Use `Arc` to allow cheap cloning and type-erased storage.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Define your custom action type
    /// struct MyAction { command: String }
    ///
    /// // Create a custom action
    /// let action = AppKeyAction::custom(MyAction { command: "foo".into() });
    ///
    /// // In your app, downcast to handle it
    /// if let AppKeyAction::Custom(any) = action {
    ///     if let Some(my_action) = any.downcast_ref::<MyAction>() {
    ///         println!("Command: {}", my_action.command);
    ///     }
    /// }
    /// ```
    Custom(Arc<dyn Any + Send + Sync>),
}

impl AppKeyAction {
    /// Create a custom action from any type that is Send + Sync + 'static.
    pub fn custom<T: Any + Send + Sync + 'static>(value: T) -> Self {
        Self::Custom(Arc::new(value))
    }
}

impl Clone for AppKeyAction {
    fn clone(&self) -> Self {
        match self {
            Self::MoveUp => Self::MoveUp,
            Self::MoveDown => Self::MoveDown,
            Self::MoveLeft => Self::MoveLeft,
            Self::MoveRight => Self::MoveRight,
            Self::MoveLineStart => Self::MoveLineStart,
            Self::MoveLineEnd => Self::MoveLineEnd,
            Self::DeleteCharBefore => Self::DeleteCharBefore,
            Self::DeleteCharAt => Self::DeleteCharAt,
            Self::KillLine => Self::KillLine,
            Self::InsertNewline => Self::InsertNewline,
            Self::InsertChar(c) => Self::InsertChar(*c),
            Self::Submit => Self::Submit,
            Self::Interrupt => Self::Interrupt,
            Self::Quit => Self::Quit,
            Self::RequestExit => Self::RequestExit,
            Self::ActivateSlashPopup => Self::ActivateSlashPopup,
            Self::Custom(arc) => Self::Custom(Arc::clone(arc)),
        }
    }
}

impl PartialEq for AppKeyAction {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::MoveUp, Self::MoveUp) => true,
            (Self::MoveDown, Self::MoveDown) => true,
            (Self::MoveLeft, Self::MoveLeft) => true,
            (Self::MoveRight, Self::MoveRight) => true,
            (Self::MoveLineStart, Self::MoveLineStart) => true,
            (Self::MoveLineEnd, Self::MoveLineEnd) => true,
            (Self::DeleteCharBefore, Self::DeleteCharBefore) => true,
            (Self::DeleteCharAt, Self::DeleteCharAt) => true,
            (Self::KillLine, Self::KillLine) => true,
            (Self::InsertNewline, Self::InsertNewline) => true,
            (Self::InsertChar(a), Self::InsertChar(b)) => a == b,
            (Self::Submit, Self::Submit) => true,
            (Self::Interrupt, Self::Interrupt) => true,
            (Self::Quit, Self::Quit) => true,
            (Self::RequestExit, Self::RequestExit) => true,
            (Self::ActivateSlashPopup, Self::ActivateSlashPopup) => true,
            (Self::Custom(a), Self::Custom(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl fmt::Debug for AppKeyAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MoveUp => write!(f, "MoveUp"),
            Self::MoveDown => write!(f, "MoveDown"),
            Self::MoveLeft => write!(f, "MoveLeft"),
            Self::MoveRight => write!(f, "MoveRight"),
            Self::MoveLineStart => write!(f, "MoveLineStart"),
            Self::MoveLineEnd => write!(f, "MoveLineEnd"),
            Self::DeleteCharBefore => write!(f, "DeleteCharBefore"),
            Self::DeleteCharAt => write!(f, "DeleteCharAt"),
            Self::KillLine => write!(f, "KillLine"),
            Self::InsertNewline => write!(f, "InsertNewline"),
            Self::InsertChar(c) => write!(f, "InsertChar({:?})", c),
            Self::Submit => write!(f, "Submit"),
            Self::Interrupt => write!(f, "Interrupt"),
            Self::Quit => write!(f, "Quit"),
            Self::RequestExit => write!(f, "RequestExit"),
            Self::ActivateSlashPopup => write!(f, "ActivateSlashPopup"),
            Self::Custom(_) => write!(f, "Custom(..)"),
        }
    }
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

    /// Create an Alt+key combination.
    pub const fn alt(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::ALT,
        }
    }

    /// Create a Shift+key combination.
    pub const fn shift(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::SHIFT,
        }
    }

    /// Create a Ctrl+Alt+key combination.
    pub const fn ctrl_alt(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL.union(KeyModifiers::ALT),
        }
    }

    /// Create a Ctrl+Shift+key combination.
    pub const fn ctrl_shift(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL.union(KeyModifiers::SHIFT),
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

    #[test]
    fn test_key_combo_alt() {
        let combo = KeyCombo::alt('x');
        let event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
        assert!(combo.matches(&event));

        let event_ctrl = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert!(!combo.matches(&event_ctrl));
    }

    #[test]
    fn test_key_combo_shift() {
        let combo = KeyCombo::shift(KeyCode::Tab);
        let event = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
        assert!(combo.matches(&event));

        let event_none = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        assert!(!combo.matches(&event_none));
    }

    #[test]
    fn test_key_combo_ctrl_alt() {
        let combo = KeyCombo::ctrl_alt('a');
        let expected_mods = KeyModifiers::CONTROL | KeyModifiers::ALT;
        let event = KeyEvent::new(KeyCode::Char('a'), expected_mods);
        assert!(combo.matches(&event));

        let event_ctrl_only = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert!(!combo.matches(&event_ctrl_only));
    }

    #[test]
    fn test_key_combo_ctrl_shift() {
        let combo = KeyCombo::ctrl_shift('z');
        let expected_mods = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        let event = KeyEvent::new(KeyCode::Char('z'), expected_mods);
        assert!(combo.matches(&event));

        let event_shift_only = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::SHIFT);
        assert!(!combo.matches(&event_shift_only));
    }

    #[test]
    fn test_custom_action_creation() {
        #[derive(Debug, Clone, PartialEq)]
        struct MyAction {
            value: i32,
        }

        let action = AppKeyAction::custom(MyAction { value: 42 });

        // Verify it can be matched and downcast
        if let AppKeyAction::Custom(any) = &action {
            let my_action = any.downcast_ref::<MyAction>().unwrap();
            assert_eq!(my_action.value, 42);
        } else {
            panic!("Expected Custom variant");
        }
    }

    #[test]
    fn test_custom_action_clone() {
        #[derive(Debug)]
        struct MyAction(#[allow(dead_code)] String);

        let action1 = AppKeyAction::custom(MyAction("test".to_string()));
        let action2 = action1.clone();

        // Both should point to the same Arc data
        if let (AppKeyAction::Custom(arc1), AppKeyAction::Custom(arc2)) = (&action1, &action2) {
            assert!(Arc::ptr_eq(arc1, arc2));
        } else {
            panic!("Expected Custom variants");
        }
    }

    #[test]
    fn test_custom_action_equality() {
        #[derive(Debug)]
        struct MyAction;

        let action1 = AppKeyAction::custom(MyAction);
        let action2 = action1.clone();
        let action3 = AppKeyAction::custom(MyAction); // Different Arc

        // Same Arc should be equal (ptr_eq)
        assert_eq!(action1, action2);

        // Different Arcs are not equal even with same type
        assert_ne!(action1, action3);
    }

    #[test]
    fn test_app_key_action_debug() {
        let custom = AppKeyAction::custom(42);
        let debug_str = format!("{:?}", custom);
        assert_eq!(debug_str, "Custom(..)");

        let quit = AppKeyAction::Quit;
        let debug_str = format!("{:?}", quit);
        assert_eq!(debug_str, "Quit");
    }
}
