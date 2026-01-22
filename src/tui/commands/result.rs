//! Command execution result type.

/// Result of executing a slash command.
#[derive(Debug, Clone)]
pub enum CommandResult {
    /// Command succeeded, no message to display.
    Ok,

    /// Command succeeded, display this message in the conversation.
    Message(String),

    /// Command failed with an error message.
    Error(String),

    /// Command requests the application to quit.
    Quit,

    /// Command handled its own UI (e.g., opened a picker).
    /// App should not display any additional message.
    Handled,
}

impl CommandResult {
    /// Create a success result with a message.
    pub fn message(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }

    /// Create an error result.
    pub fn error(msg: impl Into<String>) -> Self {
        Self::Error(msg.into())
    }
}
