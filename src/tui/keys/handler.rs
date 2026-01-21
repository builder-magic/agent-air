//! Key handler trait and default implementation.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::bindings::KeyBindings;
use super::exit::ExitState;
use super::types::{AppKeyAction, AppKeyResult, KeyContext};

/// Trait for customizing key handling at the App level.
///
/// Implement this to customize how keys are processed BEFORE
/// they reach widgets or default text input handling.
///
/// # Key Flow
///
/// ```text
/// Key Press
///     |
/// KeyHandler.handle_key(key, context)  <- context.widget_blocking tells if modal is open
///     |
/// If NotHandled -> Widget dispatch (modals like QuestionPanel get the key)
///     |
/// If still unhandled -> Default text input handling
/// ```
pub trait KeyHandler: Send + 'static {
    /// Handle a key event.
    ///
    /// Called for every key press. Return:
    /// - `NotHandled` to pass to widgets and default handling
    /// - `Handled` to consume the key
    /// - `Action(...)` to execute an app action
    ///
    /// # Arguments
    /// * `key` - The key event
    /// * `context` - Current app context (input state, processing state, etc.)
    fn handle_key(&mut self, key: KeyEvent, context: &KeyContext) -> AppKeyResult;

    /// Get a status hint to display in the status bar.
    ///
    /// This allows the handler to provide context-sensitive hints,
    /// such as "Press again to exit" when in exit confirmation mode.
    fn status_hint(&self) -> Option<String> {
        None
    }
}

/// Default key handler with configurable bindings.
///
/// This implementation uses [`KeyBindings`] to determine what actions
/// to take for each key press. It handles the standard key processing
/// flow while allowing customization of all bindings.
///
/// The handler manages exit confirmation state internally, so agents
/// get the two-key exit flow (e.g., press Ctrl+D twice) without needing
/// to track any state in the App.
pub struct DefaultKeyHandler {
    bindings: KeyBindings,
    exit_state: ExitState,
}

impl DefaultKeyHandler {
    /// Create a new handler with the given bindings.
    pub fn new(bindings: KeyBindings) -> Self {
        Self {
            bindings,
            exit_state: ExitState::default(),
        }
    }

    /// Check if the given key matches the exit mode binding.
    fn is_exit_key(&self, key: &KeyEvent) -> bool {
        KeyBindings::matches_any(&self.bindings.enter_exit_mode, key)
    }
}

impl Default for DefaultKeyHandler {
    fn default() -> Self {
        Self::new(KeyBindings::default())
    }
}

impl KeyHandler for DefaultKeyHandler {
    fn handle_key(&mut self, key: KeyEvent, context: &KeyContext) -> AppKeyResult {
        // Check if exit confirmation has expired
        if self.exit_state.is_expired() {
            self.exit_state.reset();
        }

        // When a modal widget is blocking, let it handle most keys.
        // Only intercept "force quit" type bindings.
        if context.widget_blocking {
            // Still allow force-quit (e.g., Ctrl+Q) even in modals
            if KeyBindings::matches_any(&self.bindings.force_quit, &key) {
                return AppKeyResult::Action(AppKeyAction::Quit);
            }
            // Let the modal widget handle everything else
            return AppKeyResult::NotHandled;
        }

        // When processing (spinner active), only allow interrupt and exit
        if context.is_processing {
            if KeyBindings::matches_any(&self.bindings.interrupt, &key) {
                return AppKeyResult::Action(AppKeyAction::Interrupt);
            }
            if KeyBindings::matches_any(&self.bindings.force_quit, &key) {
                return AppKeyResult::Action(AppKeyAction::Quit);
            }
            // Handle exit mode: exit key to enter or confirm exit
            if self.is_exit_key(&key) {
                if self.exit_state.is_awaiting() {
                    self.exit_state.reset();
                    return AppKeyResult::Action(AppKeyAction::RequestExit);
                } else if context.input_empty {
                    self.exit_state = ExitState::awaiting_confirmation(
                        self.bindings.exit_timeout_secs,
                    );
                    return AppKeyResult::Handled;
                }
            }
            // Ignore all other keys during processing
            return AppKeyResult::Handled;
        }

        // Check for exit mode confirmation (handler manages this internally)
        if self.exit_state.is_awaiting() {
            if self.is_exit_key(&key) {
                self.exit_state.reset();
                return AppKeyResult::Action(AppKeyAction::RequestExit);
            }
            // Any other key cancels exit mode
            self.exit_state.reset();
            // Fall through to normal handling
        }

        // Application-level bindings
        if KeyBindings::matches_any(&self.bindings.force_quit, &key) {
            return AppKeyResult::Action(AppKeyAction::Quit);
        }
        if KeyBindings::matches_any(&self.bindings.quit, &key) && context.input_empty {
            return AppKeyResult::Action(AppKeyAction::Quit);
        }
        if self.is_exit_key(&key) {
            if context.input_empty {
                // Enter exit confirmation mode
                self.exit_state = ExitState::awaiting_confirmation(
                    self.bindings.exit_timeout_secs,
                );
                return AppKeyResult::Handled;
            }
            // When not empty, Ctrl+D is delete char at cursor
            return AppKeyResult::Action(AppKeyAction::DeleteCharAt);
        }
        if KeyBindings::matches_any(&self.bindings.submit, &key) {
            return AppKeyResult::Action(AppKeyAction::Submit);
        }
        if KeyBindings::matches_any(&self.bindings.interrupt, &key) {
            return AppKeyResult::Action(AppKeyAction::Interrupt);
        }

        // Navigation bindings
        if KeyBindings::matches_any(&self.bindings.move_up, &key) {
            return AppKeyResult::Action(AppKeyAction::MoveUp);
        }
        if KeyBindings::matches_any(&self.bindings.move_down, &key) {
            return AppKeyResult::Action(AppKeyAction::MoveDown);
        }
        if KeyBindings::matches_any(&self.bindings.move_left, &key) {
            return AppKeyResult::Action(AppKeyAction::MoveLeft);
        }
        if KeyBindings::matches_any(&self.bindings.move_right, &key) {
            return AppKeyResult::Action(AppKeyAction::MoveRight);
        }
        if KeyBindings::matches_any(&self.bindings.move_line_start, &key) {
            return AppKeyResult::Action(AppKeyAction::MoveLineStart);
        }
        if KeyBindings::matches_any(&self.bindings.move_line_end, &key) {
            return AppKeyResult::Action(AppKeyAction::MoveLineEnd);
        }

        // Editing bindings
        if KeyBindings::matches_any(&self.bindings.delete_char_before, &key) {
            return AppKeyResult::Action(AppKeyAction::DeleteCharBefore);
        }
        if KeyBindings::matches_any(&self.bindings.delete_char_at, &key) {
            return AppKeyResult::Action(AppKeyAction::DeleteCharAt);
        }
        if KeyBindings::matches_any(&self.bindings.kill_line, &key) {
            return AppKeyResult::Action(AppKeyAction::KillLine);
        }
        if KeyBindings::matches_any(&self.bindings.insert_newline, &key) {
            return AppKeyResult::Action(AppKeyAction::InsertNewline);
        }

        // Character input - return InsertChar for regular characters
        if let KeyCode::Char(c) = key.code {
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                return AppKeyResult::Action(AppKeyAction::InsertChar(c));
            }
        }

        // Unhandled - let widgets or default handling take over
        AppKeyResult::NotHandled
    }

    fn status_hint(&self) -> Option<String> {
        if self.exit_state.is_awaiting() {
            Some("Press again to exit".to_string())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_handler_force_quit_in_modal() {
        let mut handler = DefaultKeyHandler::default();
        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: true, // Modal is open
        };

        // Ctrl+Q should still work when modal is blocking
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        let result = handler.handle_key(key, &context);
        assert_eq!(result, AppKeyResult::Action(AppKeyAction::Quit));

        // Regular Esc should not be handled (let modal handle it)
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = handler.handle_key(esc, &context);
        assert_eq!(result, AppKeyResult::NotHandled);
    }

    #[test]
    fn test_emacs_handler_processing_mode() {
        // Use emacs bindings which have interrupt on Esc
        let mut handler = DefaultKeyHandler::new(KeyBindings::emacs());
        let context = KeyContext {
            input_empty: true,
            is_processing: true, // Spinner is active
            widget_blocking: false,
        };

        // Esc should interrupt (emacs has interrupt binding)
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = handler.handle_key(esc, &context);
        assert_eq!(result, AppKeyResult::Action(AppKeyAction::Interrupt));

        // Regular keys should be consumed (Handled)
        let a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let result = handler.handle_key(a, &context);
        assert_eq!(result, AppKeyResult::Handled);
    }

    #[test]
    fn test_emacs_handler_exit_mode() {
        // Use emacs bindings which have Ctrl+D for exit mode
        let mut handler = DefaultKeyHandler::new(KeyBindings::emacs());
        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };

        // First Ctrl+D enters exit mode, returns Handled
        let ctrl_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let result = handler.handle_key(ctrl_d, &context);
        assert_eq!(result, AppKeyResult::Handled);

        // Handler should now show status hint
        assert!(handler.status_hint().is_some());

        // Second Ctrl+D should request exit
        let result = handler.handle_key(ctrl_d, &context);
        assert_eq!(result, AppKeyResult::Action(AppKeyAction::RequestExit));

        // Status hint should be cleared
        assert!(handler.status_hint().is_none());
    }

    #[test]
    fn test_bare_minimum_handler_quit() {
        let mut handler = DefaultKeyHandler::default(); // Uses bare_minimum
        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };

        // Esc should quit when input is empty (bare_minimum has no exit mode)
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = handler.handle_key(esc, &context);
        assert_eq!(result, AppKeyResult::Action(AppKeyAction::Quit));
    }

    #[test]
    fn test_default_handler_char_input() {
        let mut handler = DefaultKeyHandler::default();
        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };

        // Regular character should be InsertChar
        let a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let result = handler.handle_key(a, &context);
        assert_eq!(result, AppKeyResult::Action(AppKeyAction::InsertChar('a')));

        // Shift+character should also be InsertChar
        let shift_a = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
        let result = handler.handle_key(shift_a, &context);
        assert_eq!(result, AppKeyResult::Action(AppKeyAction::InsertChar('A')));
    }

    #[test]
    fn test_exit_mode_cancelled_by_other_key() {
        // Use emacs bindings which have Ctrl+D for exit mode
        let mut handler = DefaultKeyHandler::new(KeyBindings::emacs());
        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };

        // First Ctrl+D enters exit mode
        let ctrl_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let result = handler.handle_key(ctrl_d, &context);
        assert_eq!(result, AppKeyResult::Handled);
        assert!(handler.status_hint().is_some());

        // Pressing another key cancels exit mode
        let a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let result = handler.handle_key(a, &context);
        assert_eq!(result, AppKeyResult::Action(AppKeyAction::InsertChar('a')));

        // Status hint should be cleared
        assert!(handler.status_hint().is_none());
    }
}
