//! Command registry for managing slash commands.

use super::standard::default_commands;
use super::traits::SlashCommand;

/// Registry of slash commands for an agent.
///
/// Provides a builder-style API for configuring which commands are available.
///
/// # Example
///
/// ```ignore
/// use agent_air::tui::commands::{
///     CommandRegistry, CustomCommand, CommandResult,
///     HelpCommand, ClearCommand, ThemesCommand,
/// };
///
/// // Start with defaults and customize
/// let commands = CommandRegistry::with_defaults()
///     .add(CustomCommand::new("deploy", "Deploy the app", |args, ctx| {
///         CommandResult::Message(format!("Deploying to {}...", args))
///     }))
///     .remove("quit")  // Disable quit command
///     .build();
///
/// // Or build from scratch
/// let commands = CommandRegistry::new()
///     .add(HelpCommand)
///     .add(ClearCommand)
///     .add(ThemesCommand)
///     .build();
/// ```
pub struct CommandRegistry {
    commands: Vec<Box<dyn SlashCommand>>,
}

impl CommandRegistry {
    /// Create an empty registry with no commands.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Create a registry with the default commands.
    ///
    /// Default commands include: help, clear, compact, themes, sessions,
    /// status, version, new-session, and quit.
    pub fn with_defaults() -> Self {
        Self {
            commands: default_commands(),
        }
    }

    /// Add a command to the registry.
    ///
    /// # Example
    ///
    /// ```ignore
    /// registry.add(ClearCommand).add(HelpCommand);
    /// ```
    #[allow(clippy::should_implement_trait)]
    pub fn add<C: SlashCommand + 'static>(mut self, command: C) -> Self {
        self.commands.push(Box::new(command));
        self
    }

    /// Add a boxed command to the registry.
    ///
    /// Use this when you already have a `Box<dyn SlashCommand>`.
    pub fn add_boxed(mut self, command: Box<dyn SlashCommand>) -> Self {
        self.commands.push(command);
        self
    }

    /// Remove a command by name.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Disable the quit command
    /// registry.remove("quit");
    /// ```
    pub fn remove(mut self, name: &str) -> Self {
        self.commands.retain(|c| c.name() != name);
        self
    }

    /// Build into the final command list.
    ///
    /// Consumes the registry and returns the commands.
    pub fn build(self) -> Vec<Box<dyn SlashCommand>> {
        self.commands
    }

    /// Get commands as a slice (for inspection without consuming).
    pub fn commands(&self) -> &[Box<dyn SlashCommand>] {
        &self.commands
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
