//! Core trait for slash commands.

use super::context::CommandContext;
use super::result::CommandResult;

/// Trait for slash commands.
///
/// Implement this for full control over command behavior, or use
/// [`CustomCommand`](super::CustomCommand) for simple closure-based commands.
///
/// # Example
///
/// ```ignore
/// use agent_air::tui::commands::{SlashCommand, CommandContext, CommandResult};
///
/// struct CounterCommand {
///     count: std::sync::atomic::AtomicUsize,
/// }
///
/// impl SlashCommand for CounterCommand {
///     fn name(&self) -> &str { "count" }
///     fn description(&self) -> &str { "Increment and show counter" }
///
///     fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
///         let n = self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
///         CommandResult::Message(format!("Count: {}", n + 1))
///     }
/// }
/// ```
pub trait SlashCommand: Send + Sync {
    /// Command name without the leading slash (e.g., "clear").
    fn name(&self) -> &str;

    /// Short description shown in the slash popup.
    fn description(&self) -> &str;

    /// Execute the command.
    ///
    /// # Arguments
    /// * `args` - Everything after the command name, trimmed
    /// * `ctx` - Context providing access to app functionality
    ///
    /// # Returns
    /// A [`CommandResult`] indicating what happened
    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult;
}
