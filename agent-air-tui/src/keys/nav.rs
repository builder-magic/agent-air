//! Navigation helper for widgets to respect configured key bindings.
//!
//! When modal widgets block input, they receive raw `KeyEvent`s and bypass the
//! `KeyBindings` system. The `NavigationHelper` wraps `KeyBindings` and provides
//! semantic key checking methods so widgets can honor user-configured bindings.

use crossterm::event::KeyEvent;

use super::bindings::KeyBindings;

/// Helper for widgets to check navigation keys against configured bindings.
///
/// Pass this to widgets so they can respect configured key bindings instead of
/// hardcoding key codes.
///
/// # Example
///
/// ```ignore
/// fn handle_key(&mut self, key: KeyEvent, ctx: &WidgetKeyContext) -> WidgetKeyResult {
///     if ctx.nav.is_move_up(&key) {
///         self.select_previous();
///         WidgetKeyResult::Handled
///     } else if ctx.nav.is_move_down(&key) {
///         self.select_next();
///         WidgetKeyResult::Handled
///     } else if ctx.nav.is_select(&key) {
///         // Handle selection
///     } else if ctx.nav.is_cancel(&key) {
///         // Handle cancel
///     } else {
///         WidgetKeyResult::NotHandled
///     }
/// }
/// ```
pub struct NavigationHelper<'a> {
    bindings: &'a KeyBindings,
}

impl<'a> NavigationHelper<'a> {
    /// Create a new navigation helper from key bindings.
    pub fn new(bindings: &'a KeyBindings) -> Self {
        Self { bindings }
    }

    /// Check if the key matches the move up binding.
    pub fn is_move_up(&self, key: &KeyEvent) -> bool {
        KeyBindings::matches_any(&self.bindings.move_up, key)
    }

    /// Check if the key matches the move down binding.
    pub fn is_move_down(&self, key: &KeyEvent) -> bool {
        KeyBindings::matches_any(&self.bindings.move_down, key)
    }

    /// Check if the key matches the move left binding.
    pub fn is_move_left(&self, key: &KeyEvent) -> bool {
        KeyBindings::matches_any(&self.bindings.move_left, key)
    }

    /// Check if the key matches the move right binding.
    pub fn is_move_right(&self, key: &KeyEvent) -> bool {
        KeyBindings::matches_any(&self.bindings.move_right, key)
    }

    /// Check if the key matches the select binding (Enter, Space).
    pub fn is_select(&self, key: &KeyEvent) -> bool {
        KeyBindings::matches_any(&self.bindings.select, key)
    }

    /// Check if the key matches the cancel binding (Esc).
    pub fn is_cancel(&self, key: &KeyEvent) -> bool {
        KeyBindings::matches_any(&self.bindings.cancel, key)
    }

    /// Check if the key matches the submit binding (Enter).
    pub fn is_submit(&self, key: &KeyEvent) -> bool {
        KeyBindings::matches_any(&self.bindings.submit, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn test_navigation_helper_emacs_bindings() {
        let bindings = KeyBindings::emacs();
        let nav = NavigationHelper::new(&bindings);

        // Ctrl+P should match move_up
        let ctrl_p = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        assert!(nav.is_move_up(&ctrl_p));

        // Up arrow should also match move_up
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert!(nav.is_move_up(&up));

        // Ctrl+N should match move_down
        let ctrl_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL);
        assert!(nav.is_move_down(&ctrl_n));

        // Down arrow should also match move_down
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert!(nav.is_move_down(&down));
    }

    #[test]
    fn test_navigation_helper_select_cancel() {
        let bindings = KeyBindings::emacs();
        let nav = NavigationHelper::new(&bindings);

        // Enter should match select
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert!(nav.is_select(&enter));

        // Space should match select
        let space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        assert!(nav.is_select(&space));

        // Esc should match cancel
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert!(nav.is_cancel(&esc));
    }

    #[test]
    fn test_navigation_helper_minimal_bindings() {
        let bindings = KeyBindings::minimal();
        let nav = NavigationHelper::new(&bindings);

        // Only arrow keys should work for navigation
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert!(nav.is_move_up(&up));

        // Ctrl+P should NOT match move_up in minimal bindings
        let ctrl_p = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        assert!(!nav.is_move_up(&ctrl_p));
    }
}
