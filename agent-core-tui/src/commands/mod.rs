//! Slash command system for the TUI.
//!
//! This module provides a trait-based command system that allows:
//! - Framework-provided standard commands (clear, help, themes, etc.)
//! - Custom commands via closures or trait implementations
//! - Agent-specific context data accessible to commands
//!
//! # Quick Start
//!
//! Use the default commands:
//!
//! ```ignore
//! use agent_core::tui::commands::CommandRegistry;
//!
//! // Use all default commands
//! let commands = CommandRegistry::with_defaults().build();
//! agent.set_commands(commands);
//! ```
//!
//! # Custom Commands
//!
//! Add custom commands alongside defaults:
//!
//! ```ignore
//! use agent_core::tui::commands::{CommandRegistry, CustomCommand, CommandResult};
//!
//! let commands = CommandRegistry::with_defaults()
//!     .add(CustomCommand::new("deploy", "Deploy the application", |args, ctx| {
//!         CommandResult::Message(format!("Deploying to {}...", args))
//!     }))
//!     .remove("quit")  // Optionally remove commands
//!     .build();
//! ```
//!
//! # Extension Context
//!
//! Provide agent-specific data to commands:
//!
//! ```ignore
//! struct MyAgentContext {
//!     api_key: String,
//!     environments: Vec<String>,
//! }
//!
//! agent.set_command_extension(MyAgentContext {
//!     api_key: "secret".into(),
//!     environments: vec!["staging".into(), "prod".into()],
//! });
//!
//! // In command:
//! CustomCommand::new("deploy", "Deploy app", |args, ctx| {
//!     let my_ctx = ctx.extension::<MyAgentContext>().expect("context required");
//!     if my_ctx.environments.contains(&args.to_string()) {
//!         CommandResult::Message(format!("Deploying to {}...", args))
//!     } else {
//!         CommandResult::Error(format!("Unknown environment: {}", args))
//!     }
//! })
//! ```

mod context;
mod custom;
mod helpers;
mod registry;
mod result;
mod standard;
mod traits;

// Re-export main types
pub use context::{CommandContext, PendingAction};
pub use custom::CustomCommand;
pub use helpers::{
    filter_commands, generate_help_message, get_command_by_name, is_slash_command, parse_command,
};
pub use registry::CommandRegistry;
pub use result::CommandResult;
pub use traits::SlashCommand;

// Re-export standard commands
pub use standard::{
    ClearCommand, CompactCommand, HelpCommand, NewSessionCommand, QuitCommand, SessionsCommand,
    StatusCommand, ThemesCommand, VersionCommand, default_commands,
};
