//! Key handler trait and default implementation.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::bindings::KeyBindings;
use super::exit::ExitState;
use super::types::{AppKeyAction, AppKeyResult, KeyCombo, KeyContext};

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

    /// Get a reference to the key bindings.
    ///
    /// This is used by widgets to check navigation keys against configured bindings.
    /// The default implementation returns `minimal` bindings.
    fn bindings(&self) -> &KeyBindings;
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
///
/// # Custom Bindings
///
/// In addition to the standard bindings, you can add custom key bindings
/// that trigger custom actions:
///
/// ```ignore
/// let handler = DefaultKeyHandler::new(KeyBindings::emacs())
///     .with_custom_binding(KeyCombo::ctrl('t'), || {
///         AppKeyAction::custom(MyCustomAction::ToggleSomething)
///     });
/// ```
pub struct DefaultKeyHandler {
    bindings: KeyBindings,
    exit_state: ExitState,
    custom_bindings: Vec<(KeyCombo, Box<dyn Fn() -> AppKeyAction + Send + Sync>)>,
}

impl DefaultKeyHandler {
    /// Create a new handler with the given bindings.
    pub fn new(bindings: KeyBindings) -> Self {
        Self {
            bindings,
            exit_state: ExitState::default(),
            custom_bindings: Vec::new(),
        }
    }

    /// Get a reference to the key bindings.
    pub fn bindings(&self) -> &KeyBindings {
        &self.bindings
    }

    /// Add a custom key binding that triggers a custom action.
    ///
    /// Custom bindings are checked before standard bindings, allowing
    /// you to override default behavior or add new key combinations.
    ///
    /// # Arguments
    ///
    /// * `combo` - The key combination to bind
    /// * `action_fn` - A function that returns the action to execute
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = DefaultKeyHandler::new(KeyBindings::emacs())
    ///     .with_custom_binding(KeyCombo::ctrl('t'), || {
    ///         AppKeyAction::custom(MyCustomAction::ToggleSomething)
    ///     })
    ///     .with_custom_binding(KeyCombo::alt('h'), || {
    ///         AppKeyAction::custom(MyCustomAction::ShowHelp)
    ///     });
    /// ```
    pub fn with_custom_binding<F>(mut self, combo: KeyCombo, action_fn: F) -> Self
    where
        F: Fn() -> AppKeyAction + Send + Sync + 'static,
    {
        self.custom_bindings.push((combo, Box::new(action_fn)));
        self
    }

    /// Check if the given key matches the exit mode binding.
    fn is_exit_key(&self, key: &KeyEvent) -> bool {
        KeyBindings::matches_any(&self.bindings.enter_exit_mode, key)
    }

    /// Check if the given key matches any custom binding.
    fn check_custom_binding(&self, key: &KeyEvent) -> Option<AppKeyAction> {
        for (combo, action_fn) in &self.custom_bindings {
            if combo.matches(key) {
                return Some(action_fn());
            }
        }
        None
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
                    self.exit_state =
                        ExitState::awaiting_confirmation(self.bindings.exit_timeout_secs);
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

        // Check custom bindings first (allows overriding standard bindings)
        if let Some(action) = self.check_custom_binding(&key) {
            return AppKeyResult::Action(action);
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
                self.exit_state = ExitState::awaiting_confirmation(self.bindings.exit_timeout_secs);
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

    fn bindings(&self) -> &KeyBindings {
        &self.bindings
    }
}

/// A composable key handler wrapper with pre-processing hooks.
///
/// `ComposedKeyHandler` wraps an inner handler and allows adding hooks
/// that run before the inner handler processes keys. This enables:
///
/// - Intercepting specific keys before they reach the inner handler
/// - Adding logging or debugging behavior
/// - Implementing layered key handling (e.g., modal modes on top of base handler)
///
/// # Example
///
/// ```ignore
/// let base_handler = DefaultKeyHandler::new(KeyBindings::emacs());
/// let handler = ComposedKeyHandler::new(base_handler)
///     .with_pre_hook(|key, _ctx| {
///         // Intercept F1 for help
///         if key.code == KeyCode::F(1) {
///             return Some(AppKeyResult::Action(AppKeyAction::custom(ShowHelp)));
///         }
///         None // Let inner handler process
///     });
/// ```
pub struct ComposedKeyHandler<H: KeyHandler> {
    inner: H,
    pre_hooks: Vec<Box<dyn Fn(&KeyEvent, &KeyContext) -> Option<AppKeyResult> + Send>>,
}

impl<H: KeyHandler> ComposedKeyHandler<H> {
    /// Create a new composed handler wrapping the given inner handler.
    pub fn new(inner: H) -> Self {
        Self {
            inner,
            pre_hooks: Vec::new(),
        }
    }

    /// Add a hook that runs before the inner handler.
    ///
    /// If the hook returns `Some(result)`, that result is used and the
    /// inner handler is skipped. If the hook returns `None`, processing
    /// continues to the next hook or the inner handler.
    ///
    /// Hooks are called in the order they were added.
    ///
    /// # Arguments
    ///
    /// * `hook` - A function that receives the key event and context,
    ///   returning `Some(AppKeyResult)` to handle the key or `None` to pass through
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = ComposedKeyHandler::new(DefaultKeyHandler::default())
    ///     .with_pre_hook(|key, ctx| {
    ///         // Log all key presses
    ///         eprintln!("Key: {:?}", key);
    ///         None // Don't consume, let inner handler process
    ///     })
    ///     .with_pre_hook(|key, _ctx| {
    ///         // Intercept Ctrl+H for custom help
    ///         if key.code == KeyCode::Char('h') && key.modifiers == KeyModifiers::CONTROL {
    ///             return Some(AppKeyResult::Action(AppKeyAction::custom(MyHelp)));
    ///         }
    ///         None
    ///     });
    /// ```
    pub fn with_pre_hook<F>(mut self, hook: F) -> Self
    where
        F: Fn(&KeyEvent, &KeyContext) -> Option<AppKeyResult> + Send + 'static,
    {
        self.pre_hooks.push(Box::new(hook));
        self
    }

    /// Get a reference to the inner handler.
    pub fn inner(&self) -> &H {
        &self.inner
    }

    /// Get a mutable reference to the inner handler.
    pub fn inner_mut(&mut self) -> &mut H {
        &mut self.inner
    }
}

impl<H: KeyHandler> KeyHandler for ComposedKeyHandler<H> {
    fn handle_key(&mut self, key: KeyEvent, context: &KeyContext) -> AppKeyResult {
        // Run pre-hooks in order
        for hook in &self.pre_hooks {
            if let Some(result) = hook(&key, context) {
                return result;
            }
        }

        // Fall through to inner handler
        self.inner.handle_key(key, context)
    }

    fn status_hint(&self) -> Option<String> {
        self.inner.status_hint()
    }

    fn bindings(&self) -> &KeyBindings {
        self.inner.bindings()
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
    fn test_minimal_handler_quit() {
        let mut handler = DefaultKeyHandler::default(); // Uses minimal
        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };

        // Esc should quit when input is empty (minimal has no exit mode)
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

    #[test]
    fn test_custom_binding_basic() {
        // Create handler with a custom binding for Ctrl+T
        let mut handler = DefaultKeyHandler::new(KeyBindings::emacs())
            .with_custom_binding(KeyCombo::ctrl('t'), || AppKeyAction::custom("toggle"));

        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };

        // Ctrl+T should trigger the custom action
        let ctrl_t = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL);
        let result = handler.handle_key(ctrl_t, &context);

        if let AppKeyResult::Action(AppKeyAction::Custom(any)) = result {
            assert!(any.downcast_ref::<&str>().is_some());
        } else {
            panic!("Expected Custom action, got {:?}", result);
        }
    }

    #[test]
    fn test_custom_binding_overrides_standard() {
        // Custom binding for Ctrl+P should override the standard move_up
        let mut handler = DefaultKeyHandler::new(KeyBindings::emacs())
            .with_custom_binding(KeyCombo::ctrl('p'), || AppKeyAction::custom("custom_up"));

        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };

        let ctrl_p = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let result = handler.handle_key(ctrl_p, &context);

        // Should get custom action, not MoveUp
        if let AppKeyResult::Action(AppKeyAction::Custom(_)) = result {
            // Good - custom binding took precedence
        } else {
            panic!(
                "Expected Custom action to override MoveUp, got {:?}",
                result
            );
        }
    }

    #[test]
    fn test_composed_handler_basic() {
        let base = DefaultKeyHandler::new(KeyBindings::minimal());
        let mut composed = ComposedKeyHandler::new(base);

        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };

        // Without hooks, should behave like the inner handler
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = composed.handle_key(esc, &context);
        assert_eq!(result, AppKeyResult::Action(AppKeyAction::Quit));
    }

    #[test]
    fn test_composed_handler_pre_hook_intercepts() {
        let base = DefaultKeyHandler::new(KeyBindings::minimal());
        let mut composed = ComposedKeyHandler::new(base).with_pre_hook(|key, _ctx| {
            // Intercept F1 key
            if key.code == KeyCode::F(1) {
                return Some(AppKeyResult::Action(AppKeyAction::custom("help")));
            }
            None
        });

        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };

        // F1 should be intercepted by the hook
        let f1 = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        let result = composed.handle_key(f1, &context);

        if let AppKeyResult::Action(AppKeyAction::Custom(_)) = result {
            // Good - hook intercepted
        } else {
            panic!("Expected hook to intercept F1, got {:?}", result);
        }

        // Other keys should pass through to inner handler
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = composed.handle_key(esc, &context);
        assert_eq!(result, AppKeyResult::Action(AppKeyAction::Quit));
    }

    #[test]
    fn test_composed_handler_multiple_hooks() {
        let base = DefaultKeyHandler::new(KeyBindings::minimal());
        let mut composed = ComposedKeyHandler::new(base)
            .with_pre_hook(|key, _ctx| {
                // First hook intercepts F1
                if key.code == KeyCode::F(1) {
                    return Some(AppKeyResult::Action(AppKeyAction::custom("first")));
                }
                None
            })
            .with_pre_hook(|key, _ctx| {
                // Second hook intercepts F2 and also F1 (but won't get F1)
                if key.code == KeyCode::F(2) {
                    return Some(AppKeyResult::Action(AppKeyAction::custom("second")));
                }
                if key.code == KeyCode::F(1) {
                    return Some(AppKeyResult::Action(AppKeyAction::custom("should_not_see")));
                }
                None
            });

        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };

        // F1 should be handled by first hook
        let f1 = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        let result = composed.handle_key(f1, &context);
        if let AppKeyResult::Action(AppKeyAction::Custom(any)) = result {
            let s = any.downcast_ref::<&str>().unwrap();
            assert_eq!(*s, "first");
        } else {
            panic!("Expected first hook to handle F1");
        }

        // F2 should be handled by second hook
        let f2 = KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE);
        let result = composed.handle_key(f2, &context);
        if let AppKeyResult::Action(AppKeyAction::Custom(any)) = result {
            let s = any.downcast_ref::<&str>().unwrap();
            assert_eq!(*s, "second");
        } else {
            panic!("Expected second hook to handle F2");
        }
    }

    #[test]
    fn test_composed_handler_status_hint() {
        // Inner handler's status_hint should be accessible
        let mut base = DefaultKeyHandler::new(KeyBindings::emacs());

        // Put base handler into exit mode
        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };
        let ctrl_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        base.handle_key(ctrl_d, &context);

        // Now wrap it
        let composed = ComposedKeyHandler::new(base);

        // Status hint should come from inner handler
        assert!(composed.status_hint().is_some());
        assert!(composed.status_hint().unwrap().contains("exit"));
    }

    #[test]
    fn test_composed_handler_inner_access() {
        let base = DefaultKeyHandler::new(KeyBindings::emacs());
        let mut composed = ComposedKeyHandler::new(base);

        // Can access inner handler
        assert!(composed.inner().status_hint().is_none());

        // Can mutate inner handler
        let context = KeyContext {
            input_empty: true,
            is_processing: false,
            widget_blocking: false,
        };
        let ctrl_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        composed.inner_mut().handle_key(ctrl_d, &context);

        // Inner state changed
        assert!(composed.inner().status_hint().is_some());
    }
}
