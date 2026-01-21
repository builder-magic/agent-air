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

    // -------------------------------------------------------------------------
    // Builder pattern methods: with_* setters
    // -------------------------------------------------------------------------

    /// Set the move up key bindings.
    pub fn with_move_up(mut self, combos: Vec<KeyCombo>) -> Self {
        self.move_up = combos;
        self
    }

    /// Set the move down key bindings.
    pub fn with_move_down(mut self, combos: Vec<KeyCombo>) -> Self {
        self.move_down = combos;
        self
    }

    /// Set the move left key bindings.
    pub fn with_move_left(mut self, combos: Vec<KeyCombo>) -> Self {
        self.move_left = combos;
        self
    }

    /// Set the move right key bindings.
    pub fn with_move_right(mut self, combos: Vec<KeyCombo>) -> Self {
        self.move_right = combos;
        self
    }

    /// Set the move to line start key bindings.
    pub fn with_move_line_start(mut self, combos: Vec<KeyCombo>) -> Self {
        self.move_line_start = combos;
        self
    }

    /// Set the move to line end key bindings.
    pub fn with_move_line_end(mut self, combos: Vec<KeyCombo>) -> Self {
        self.move_line_end = combos;
        self
    }

    /// Set the delete char before (backspace) key bindings.
    pub fn with_delete_char_before(mut self, combos: Vec<KeyCombo>) -> Self {
        self.delete_char_before = combos;
        self
    }

    /// Set the delete char at (delete) key bindings.
    pub fn with_delete_char_at(mut self, combos: Vec<KeyCombo>) -> Self {
        self.delete_char_at = combos;
        self
    }

    /// Set the kill line key bindings.
    pub fn with_kill_line(mut self, combos: Vec<KeyCombo>) -> Self {
        self.kill_line = combos;
        self
    }

    /// Set the insert newline key bindings.
    pub fn with_insert_newline(mut self, combos: Vec<KeyCombo>) -> Self {
        self.insert_newline = combos;
        self
    }

    /// Set the submit key bindings.
    pub fn with_submit(mut self, combos: Vec<KeyCombo>) -> Self {
        self.submit = combos;
        self
    }

    /// Set the interrupt key bindings.
    pub fn with_interrupt(mut self, combos: Vec<KeyCombo>) -> Self {
        self.interrupt = combos;
        self
    }

    /// Set the quit key bindings.
    pub fn with_quit(mut self, combos: Vec<KeyCombo>) -> Self {
        self.quit = combos;
        self
    }

    /// Set the force quit key bindings.
    pub fn with_force_quit(mut self, combos: Vec<KeyCombo>) -> Self {
        self.force_quit = combos;
        self
    }

    /// Set the enter exit mode key bindings.
    pub fn with_enter_exit_mode(mut self, combos: Vec<KeyCombo>) -> Self {
        self.enter_exit_mode = combos;
        self
    }

    /// Set the exit timeout in seconds.
    pub fn with_exit_timeout_secs(mut self, secs: u64) -> Self {
        self.exit_timeout_secs = secs;
        self
    }

    // -------------------------------------------------------------------------
    // Builder pattern methods: without_* for disabling
    // -------------------------------------------------------------------------

    /// Disable exit mode (sets enter_exit_mode to empty).
    pub fn without_exit_mode(mut self) -> Self {
        self.enter_exit_mode = vec![];
        self
    }

    /// Disable quit binding (sets quit to empty).
    pub fn without_quit(mut self) -> Self {
        self.quit = vec![];
        self
    }

    /// Disable force quit binding (sets force_quit to empty).
    pub fn without_force_quit(mut self) -> Self {
        self.force_quit = vec![];
        self
    }

    /// Disable interrupt binding (sets interrupt to empty).
    pub fn without_interrupt(mut self) -> Self {
        self.interrupt = vec![];
        self
    }

    /// Disable kill line binding (sets kill_line to empty).
    pub fn without_kill_line(mut self) -> Self {
        self.kill_line = vec![];
        self
    }

    /// Disable insert newline binding (sets insert_newline to empty).
    pub fn without_insert_newline(mut self) -> Self {
        self.insert_newline = vec![];
        self
    }

    // -------------------------------------------------------------------------
    // Builder pattern methods: add_* for appending
    // -------------------------------------------------------------------------

    /// Add a key combo to the move up bindings.
    pub fn add_move_up(mut self, combo: KeyCombo) -> Self {
        self.move_up.push(combo);
        self
    }

    /// Add a key combo to the move down bindings.
    pub fn add_move_down(mut self, combo: KeyCombo) -> Self {
        self.move_down.push(combo);
        self
    }

    /// Add a key combo to the move left bindings.
    pub fn add_move_left(mut self, combo: KeyCombo) -> Self {
        self.move_left.push(combo);
        self
    }

    /// Add a key combo to the move right bindings.
    pub fn add_move_right(mut self, combo: KeyCombo) -> Self {
        self.move_right.push(combo);
        self
    }

    /// Add a key combo to the quit bindings.
    pub fn add_quit(mut self, combo: KeyCombo) -> Self {
        self.quit.push(combo);
        self
    }

    /// Add a key combo to the submit bindings.
    pub fn add_submit(mut self, combo: KeyCombo) -> Self {
        self.submit.push(combo);
        self
    }

    /// Add a key combo to the interrupt bindings.
    pub fn add_interrupt(mut self, combo: KeyCombo) -> Self {
        self.interrupt.push(combo);
        self
    }

    /// Add a key combo to the enter exit mode bindings.
    pub fn add_enter_exit_mode(mut self, combo: KeyCombo) -> Self {
        self.enter_exit_mode.push(combo);
        self
    }

    /// Add a key combo to the force quit bindings.
    pub fn add_force_quit(mut self, combo: KeyCombo) -> Self {
        self.force_quit.push(combo);
        self
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

    #[test]
    fn test_builder_with_methods() {
        // Start with minimal and override some bindings
        let bindings = KeyBindings::minimal()
            .with_quit(vec![KeyCombo::ctrl('w')])
            .with_submit(vec![KeyCombo::key(KeyCode::Enter), KeyCombo::ctrl('m')]);

        // quit should be replaced with Ctrl+W
        assert_eq!(bindings.quit.len(), 1);
        assert!(bindings.quit.contains(&KeyCombo::ctrl('w')));

        // submit should have both Enter and Ctrl+M
        assert_eq!(bindings.submit.len(), 2);
        assert!(bindings.submit.contains(&KeyCombo::key(KeyCode::Enter)));
        assert!(bindings.submit.contains(&KeyCombo::ctrl('m')));
    }

    #[test]
    fn test_builder_without_methods() {
        // Start with emacs and disable some features
        let bindings = KeyBindings::emacs()
            .without_exit_mode()
            .without_kill_line();

        // Exit mode should be empty
        assert!(bindings.enter_exit_mode.is_empty());

        // Kill line should be empty
        assert!(bindings.kill_line.is_empty());

        // Other bindings should still exist
        assert!(!bindings.move_up.is_empty());
        assert!(!bindings.submit.is_empty());
    }

    #[test]
    fn test_builder_add_methods() {
        // Start with minimal and add extra bindings
        let bindings = KeyBindings::minimal()
            .add_quit(KeyCombo::ctrl('c'))
            .add_submit(KeyCombo::ctrl('s'));

        // quit should have both Esc (original) and Ctrl+C (added)
        assert!(bindings.quit.contains(&KeyCombo::key(KeyCode::Esc)));
        assert!(bindings.quit.contains(&KeyCombo::ctrl('c')));

        // submit should have both Enter (original) and Ctrl+S (added)
        assert!(bindings.submit.contains(&KeyCombo::key(KeyCode::Enter)));
        assert!(bindings.submit.contains(&KeyCombo::ctrl('s')));
    }

    #[test]
    fn test_builder_chaining() {
        // Test a complex chain of builder methods
        let bindings = KeyBindings::bare_minimum()
            .with_move_up(vec![KeyCombo::key(KeyCode::Up), KeyCombo::ctrl('p')])
            .with_move_down(vec![KeyCombo::key(KeyCode::Down), KeyCombo::ctrl('n')])
            .without_quit()
            .with_enter_exit_mode(vec![KeyCombo::ctrl('d')])
            .with_exit_timeout_secs(5)
            .add_force_quit(KeyCombo::ctrl('c'));

        // Verify all customizations
        assert!(bindings.move_up.contains(&KeyCombo::ctrl('p')));
        assert!(bindings.move_down.contains(&KeyCombo::ctrl('n')));
        assert!(bindings.quit.is_empty());
        assert!(bindings.enter_exit_mode.contains(&KeyCombo::ctrl('d')));
        assert_eq!(bindings.exit_timeout_secs, 5);
        assert!(bindings.force_quit.contains(&KeyCombo::ctrl('c')));
    }
}
