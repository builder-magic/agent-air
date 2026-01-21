//! Key binding presets and configuration.

use crossterm::event::{KeyCode, KeyEvent};

use super::types::KeyCombo;

/// Default exit confirmation timeout in seconds.
pub const DEFAULT_EXIT_TIMEOUT_SECS: u64 = 2;

/// Key binding configuration.
///
/// Specifies which key combinations trigger which actions.
/// Multiple key combinations can be assigned to the same action.
///
/// The default is [`bare_minimum()`](Self::bare_minimum) which only provides
/// basic functionality. Apps should explicitly choose their bindings:
/// - [`emacs()`](Self::emacs) for full Emacs-style bindings
/// - [`minimal()`](Self::minimal) for simple arrow-key navigation
#[derive(Debug, Clone)]
pub struct KeyBindings {
    // Navigation
    /// Move cursor up.
    pub move_up: Vec<KeyCombo>,
    /// Move cursor down.
    pub move_down: Vec<KeyCombo>,
    /// Move cursor left.
    pub move_left: Vec<KeyCombo>,
    /// Move cursor right.
    pub move_right: Vec<KeyCombo>,
    /// Move to line start.
    pub move_line_start: Vec<KeyCombo>,
    /// Move to line end.
    pub move_line_end: Vec<KeyCombo>,

    // Editing
    /// Delete char before cursor.
    pub delete_char_before: Vec<KeyCombo>,
    /// Delete char at cursor.
    pub delete_char_at: Vec<KeyCombo>,
    /// Kill to end of line.
    pub kill_line: Vec<KeyCombo>,
    /// Insert newline in multi-line input.
    pub insert_newline: Vec<KeyCombo>,

    // Application
    /// Submit message.
    pub submit: Vec<KeyCombo>,
    /// Interrupt current request.
    pub interrupt: Vec<KeyCombo>,
    /// Quit immediately (only when input empty and no modal is blocking).
    pub quit: Vec<KeyCombo>,
    /// Force quit (works even in modals).
    pub force_quit: Vec<KeyCombo>,
    /// Enter exit confirmation mode (requires pressing twice to exit).
    pub enter_exit_mode: Vec<KeyCombo>,
    /// Timeout in seconds for exit confirmation mode.
    pub exit_timeout_secs: u64,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::bare_minimum()
    }
}

impl KeyBindings {
    /// Bare minimum bindings - only Esc to quit.
    ///
    /// This is the default when no bindings are specified.
    /// Apps should explicitly choose their bindings (e.g., `emacs()` or `minimal()`).
    ///
    /// Only provides:
    /// - Esc to quit (when input is empty)
    /// - Ctrl+Q force quit (always works)
    /// - Enter to submit
    /// - Backspace/Delete for basic editing
    /// - Arrow keys for navigation
    pub fn bare_minimum() -> Self {
        Self {
            move_up: vec![KeyCombo::key(KeyCode::Up)],
            move_down: vec![KeyCombo::key(KeyCode::Down)],
            move_left: vec![KeyCombo::key(KeyCode::Left)],
            move_right: vec![KeyCombo::key(KeyCode::Right)],
            move_line_start: vec![KeyCombo::key(KeyCode::Home)],
            move_line_end: vec![KeyCombo::key(KeyCode::End)],

            delete_char_before: vec![KeyCombo::key(KeyCode::Backspace)],
            delete_char_at: vec![KeyCombo::key(KeyCode::Delete)],
            kill_line: vec![],
            insert_newline: vec![],

            submit: vec![KeyCombo::key(KeyCode::Enter)],
            interrupt: vec![],
            quit: vec![KeyCombo::key(KeyCode::Esc)], // Esc quits when input empty
            force_quit: vec![KeyCombo::ctrl('q')],
            enter_exit_mode: vec![],
            exit_timeout_secs: DEFAULT_EXIT_TIMEOUT_SECS,
        }
    }

    /// Emacs-style bindings.
    ///
    /// Full-featured bindings for power users:
    /// - Ctrl+P/N/B/F for navigation
    /// - Ctrl+A/E for line start/end
    /// - Ctrl+K to kill line
    /// - Ctrl+D for exit mode (or delete char if not empty)
    /// - Esc to interrupt
    pub fn emacs() -> Self {
        Self {
            move_up: vec![KeyCombo::key(KeyCode::Up), KeyCombo::ctrl('p')],
            move_down: vec![KeyCombo::key(KeyCode::Down), KeyCombo::ctrl('n')],
            move_left: vec![KeyCombo::key(KeyCode::Left), KeyCombo::ctrl('b')],
            move_right: vec![KeyCombo::key(KeyCode::Right), KeyCombo::ctrl('f')],
            move_line_start: vec![KeyCombo::key(KeyCode::Home), KeyCombo::ctrl('a')],
            move_line_end: vec![KeyCombo::key(KeyCode::End), KeyCombo::ctrl('e')],

            delete_char_before: vec![KeyCombo::key(KeyCode::Backspace)],
            delete_char_at: vec![KeyCombo::key(KeyCode::Delete)],
            kill_line: vec![KeyCombo::ctrl('k')],
            insert_newline: vec![KeyCombo::ctrl('j')],

            submit: vec![KeyCombo::key(KeyCode::Enter)],
            interrupt: vec![KeyCombo::key(KeyCode::Esc)],
            quit: vec![], // No direct quit, use exit mode
            force_quit: vec![KeyCombo::ctrl('q')],
            enter_exit_mode: vec![KeyCombo::ctrl('d')],
            exit_timeout_secs: DEFAULT_EXIT_TIMEOUT_SECS,
        }
    }

    /// Minimal bindings (arrows only, Esc to quit).
    ///
    /// This is simpler for users unfamiliar with Emacs:
    /// - Arrow keys only for navigation
    /// - Esc quits (when input empty and no modal)
    /// - No Ctrl key requirements for basic use
    pub fn minimal() -> Self {
        Self {
            move_up: vec![KeyCombo::key(KeyCode::Up)],
            move_down: vec![KeyCombo::key(KeyCode::Down)],
            move_left: vec![KeyCombo::key(KeyCode::Left)],
            move_right: vec![KeyCombo::key(KeyCode::Right)],
            move_line_start: vec![KeyCombo::key(KeyCode::Home)],
            move_line_end: vec![KeyCombo::key(KeyCode::End)],

            delete_char_before: vec![KeyCombo::key(KeyCode::Backspace)],
            delete_char_at: vec![KeyCombo::key(KeyCode::Delete)],
            kill_line: vec![],
            insert_newline: vec![KeyCombo::ctrl('j')],

            submit: vec![KeyCombo::key(KeyCode::Enter)],
            interrupt: vec![],
            quit: vec![KeyCombo::key(KeyCode::Esc)], // Esc quits (when no modal)
            force_quit: vec![KeyCombo::ctrl('q')],   // Ctrl+Q always works
            enter_exit_mode: vec![],
            exit_timeout_secs: DEFAULT_EXIT_TIMEOUT_SECS,
        }
    }

    /// Check if any combo in a list matches the key event.
    pub(crate) fn matches_any(combos: &[KeyCombo], event: &KeyEvent) -> bool {
        combos.iter().any(|combo| combo.matches(event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emacs_bindings() {
        let bindings = KeyBindings::emacs();

        // Ctrl+P should be in move_up
        let ctrl_p = KeyCombo::ctrl('p');
        assert!(bindings.move_up.contains(&ctrl_p));

        // Up arrow should also be in move_up
        let up = KeyCombo::key(KeyCode::Up);
        assert!(bindings.move_up.contains(&up));

        // quit should be empty (use exit mode instead)
        assert!(bindings.quit.is_empty());
    }

    #[test]
    fn test_minimal_bindings() {
        let bindings = KeyBindings::minimal();

        // Esc should quit (not just interrupt)
        let esc = KeyCombo::key(KeyCode::Esc);
        assert!(bindings.quit.contains(&esc));

        // No Emacs bindings
        let ctrl_p = KeyCombo::ctrl('p');
        assert!(!bindings.move_up.contains(&ctrl_p));
    }

    #[test]
    fn test_bare_minimum_bindings() {
        let bindings = KeyBindings::bare_minimum();

        // Esc should quit (not interrupt)
        let esc = KeyCombo::key(KeyCode::Esc);
        assert!(bindings.quit.contains(&esc));
        assert!(bindings.interrupt.is_empty());

        // No Emacs bindings
        let ctrl_p = KeyCombo::ctrl('p');
        assert!(!bindings.move_up.contains(&ctrl_p));

        // No exit mode
        assert!(bindings.enter_exit_mode.is_empty());
    }
}
