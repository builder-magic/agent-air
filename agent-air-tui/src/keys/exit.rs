//! Exit handling for TUI applications.
//!
//! This module provides:
//! - [`ExitHandler`] trait for agent cleanup on exit
//! - [`ExitState`] enum for tracking exit confirmation state

use std::time::Instant;

/// Exit confirmation state for key handlers.
///
/// This tracks whether the user has initiated an exit sequence
/// that requires confirmation (e.g., press Ctrl+D twice to quit).
#[derive(Debug, Clone, Default)]
pub enum ExitState {
    /// Normal operation, no exit pending.
    #[default]
    Normal,
    /// Awaiting exit confirmation within the timeout.
    AwaitingConfirmation {
        /// When the exit sequence was initiated.
        since: Instant,
        /// Timeout in seconds for confirmation.
        timeout_secs: u64,
    },
}

impl ExitState {
    /// Create a new exit confirmation state.
    pub fn awaiting_confirmation(timeout_secs: u64) -> Self {
        Self::AwaitingConfirmation {
            since: Instant::now(),
            timeout_secs,
        }
    }

    /// Check if the confirmation has expired.
    pub fn is_expired(&self) -> bool {
        match self {
            Self::Normal => false,
            Self::AwaitingConfirmation {
                since,
                timeout_secs,
            } => since.elapsed().as_secs() >= *timeout_secs,
        }
    }

    /// Check if awaiting confirmation (and not expired).
    pub fn is_awaiting(&self) -> bool {
        match self {
            Self::Normal => false,
            Self::AwaitingConfirmation {
                since,
                timeout_secs,
            } => since.elapsed().as_secs() < *timeout_secs,
        }
    }

    /// Reset to normal state.
    pub fn reset(&mut self) {
        *self = Self::Normal;
    }
}

/// Optional hook for agent cleanup on exit.
///
/// Implement this trait to run cleanup code before the application exits.
/// This is useful for saving session state, closing connections, or other
/// cleanup tasks.
///
/// # Example
///
/// ```ignore
/// struct SaveOnExitHandler {
///     session_file: PathBuf,
/// }
///
/// impl ExitHandler for SaveOnExitHandler {
///     fn on_exit(&mut self) -> bool {
///         // Save session state
///         self.save_session();
///         true // proceed with exit
///     }
/// }
/// ```
pub trait ExitHandler: Send + 'static {
    /// Called when exit is confirmed.
    ///
    /// Return `true` to proceed with exit, or `false` to cancel the exit.
    /// This allows handlers to veto the exit if needed (e.g., unsaved changes).
    fn on_exit(&mut self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_exit_state_default() {
        let state = ExitState::default();
        assert!(!state.is_awaiting());
        assert!(!state.is_expired());
    }

    #[test]
    fn test_exit_state_awaiting() {
        let state = ExitState::awaiting_confirmation(2);
        assert!(state.is_awaiting());
        assert!(!state.is_expired());
    }

    #[test]
    fn test_exit_state_expired() {
        let state = ExitState::AwaitingConfirmation {
            since: Instant::now() - Duration::from_secs(3),
            timeout_secs: 2,
        };
        assert!(!state.is_awaiting());
        assert!(state.is_expired());
    }

    #[test]
    fn test_exit_state_reset() {
        let mut state = ExitState::awaiting_confirmation(2);
        assert!(state.is_awaiting());
        state.reset();
        assert!(!state.is_awaiting());
    }
}
