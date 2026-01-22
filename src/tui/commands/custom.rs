//! Custom command helper for closure-based commands.

use super::context::CommandContext;
use super::result::CommandResult;
use super::traits::SlashCommand;

/// A simple custom command using a closure.
///
/// Use this for commands that don't need internal state.
/// For stateful commands, implement [`SlashCommand`] directly.
///
/// # Example
///
/// ```ignore
/// use agent_core::tui::commands::{CustomCommand, CommandResult};
///
/// let cmd = CustomCommand::new(
///     "greet",
///     "Say hello to someone",
///     |args, ctx| {
///         let name = if args.is_empty() { "World" } else { args };
///         CommandResult::Message(format!("Hello, {}!", name))
///     },
/// );
/// ```
pub struct CustomCommand {
    name: String,
    description: String,
    handler: Box<dyn Fn(&str, &mut CommandContext) -> CommandResult + Send + Sync>,
}

impl CustomCommand {
    /// Create a new custom command.
    ///
    /// # Arguments
    /// * `name` - Command name without the leading slash
    /// * `description` - Short description shown in the popup
    /// * `handler` - Closure that executes the command
    pub fn new<F>(name: impl Into<String>, description: impl Into<String>, handler: F) -> Self
    where
        F: Fn(&str, &mut CommandContext) -> CommandResult + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            description: description.into(),
            handler: Box::new(handler),
        }
    }
}

impl SlashCommand for CustomCommand {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        (self.handler)(args, ctx)
    }
}
