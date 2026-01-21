//! Customizable key handling for LLM TUI applications.
//!
//! This module provides a flexible key handling system that allows agents
//! to customize keyboard bindings while providing sensible defaults.
//!
//! # Overview
//!
//! The key handling system consists of:
//! - [`KeyHandler`] trait for full control over key processing
//! - [`DefaultKeyHandler`] implementation with configurable bindings
//! - [`KeyBindings`] for specifying which keys trigger which actions
//! - [`KeyCombo`] for representing key combinations (key + modifiers)
//!
//! # Examples
//!
//! ## Using preset bindings
//!
//! ```ignore
//! let core = AgentCore::new(&config)?
//!     .with_key_bindings(KeyBindings::minimal());
//! ```
//!
//! ## Customizing specific bindings
//!
//! ```ignore
//! let mut bindings = KeyBindings::emacs();
//! bindings.quit = vec![KeyCombo::key(KeyCode::Esc)];
//! bindings.enter_exit_mode = vec![]; // Disable Ctrl+D exit mode
//! let core = AgentCore::new(&config)?
//!     .with_key_bindings(bindings);
//! ```
//!
//! ## Full custom handler
//!
//! ```ignore
//! struct VimKeyHandler { mode: VimMode }
//! impl KeyHandler for VimKeyHandler {
//!     fn handle_key(&mut self, key: KeyEvent, ctx: &KeyContext) -> AppKeyResult {
//!         // Implement vim-style modal editing
//!     }
//! }
//! let core = AgentCore::new(&config)?
//!     .with_key_handler(VimKeyHandler { mode: VimMode::Normal });
//! ```

mod bindings;
mod exit;
mod handler;
mod types;

// Re-export all public types
pub use bindings::KeyBindings;
pub use exit::{ExitHandler, ExitState};
pub use handler::{ComposedKeyHandler, DefaultKeyHandler, KeyHandler};
pub use types::{AppKeyAction, AppKeyResult, KeyCombo, KeyContext};
