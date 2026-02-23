//! Framework-provided standard commands.
//!
//! These commands provide common functionality that most agents will want.
//! Use them directly or as examples for custom commands.

use super::context::CommandContext;
use super::result::CommandResult;
use super::traits::SlashCommand;

// =============================================================================
// Help Command
// =============================================================================

/// Show available commands and usage.
pub struct HelpCommand;

impl SlashCommand for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }

    fn description(&self) -> &str {
        "Show available commands and usage"
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let mut help = String::from("Available commands:\n\n");
        for cmd in ctx.commands() {
            help.push_str(&format!("  /{} - {}\n", cmd.name(), cmd.description()));
        }
        CommandResult::Message(help)
    }
}

// =============================================================================
// Clear Command
// =============================================================================

/// Clear the conversation history.
pub struct ClearCommand;

impl SlashCommand for ClearCommand {
    fn name(&self) -> &str {
        "clear"
    }

    fn description(&self) -> &str {
        "Clear the conversation history"
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        ctx.clear_conversation();
        CommandResult::Handled
    }
}

// =============================================================================
// Compact Command
// =============================================================================

/// Compact the conversation history.
pub struct CompactCommand;

impl SlashCommand for CompactCommand {
    fn name(&self) -> &str {
        "compact"
    }

    fn description(&self) -> &str {
        "Compact the conversation history"
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        ctx.compact_conversation();
        CommandResult::Handled
    }
}

// =============================================================================
// Themes Command
// =============================================================================

/// Open the theme picker to change colors.
pub struct ThemesCommand;

impl SlashCommand for ThemesCommand {
    fn name(&self) -> &str {
        "themes"
    }

    fn description(&self) -> &str {
        "Open theme picker to change colors"
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        ctx.open_theme_picker();
        CommandResult::Handled
    }
}

// =============================================================================
// Sessions Command
// =============================================================================

/// View and switch between sessions.
pub struct SessionsCommand;

impl SlashCommand for SessionsCommand {
    fn name(&self) -> &str {
        "sessions"
    }

    fn description(&self) -> &str {
        "View and switch between sessions"
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        ctx.open_session_picker();
        CommandResult::Handled
    }
}

// =============================================================================
// Status Command
// =============================================================================

/// Show current session status.
pub struct StatusCommand;

impl SlashCommand for StatusCommand {
    fn name(&self) -> &str {
        "status"
    }

    fn description(&self) -> &str {
        "Show current session status"
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        ctx.open_status_pane();
        CommandResult::Handled
    }
}

// =============================================================================
// Version Command
// =============================================================================

/// Show application version.
pub struct VersionCommand;

impl SlashCommand for VersionCommand {
    fn name(&self) -> &str {
        "version"
    }

    fn description(&self) -> &str {
        "Show application version"
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        CommandResult::Message(format!("{} v{}", ctx.agent_name(), ctx.version()))
    }
}

// =============================================================================
// Quit Command
// =============================================================================

/// Exit the application.
pub struct QuitCommand;

impl SlashCommand for QuitCommand {
    fn name(&self) -> &str {
        "quit"
    }

    fn description(&self) -> &str {
        "Exit the application"
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        ctx.request_quit();
        CommandResult::Quit
    }
}

// =============================================================================
// New Session Command
// =============================================================================

/// Create a new LLM session.
pub struct NewSessionCommand;

impl SlashCommand for NewSessionCommand {
    fn name(&self) -> &str {
        "new-session"
    }

    fn description(&self) -> &str {
        "Create a new LLM session"
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        ctx.create_new_session();
        CommandResult::Handled
    }
}

// =============================================================================
// Default Commands
// =============================================================================

/// Returns the standard set of commands most agents will want.
///
/// Includes: help, clear, compact, themes, sessions, status, version,
/// new-session, and quit.
pub fn default_commands() -> Vec<Box<dyn SlashCommand>> {
    vec![
        Box::new(HelpCommand),
        Box::new(ClearCommand),
        Box::new(CompactCommand),
        Box::new(ThemesCommand),
        Box::new(SessionsCommand),
        Box::new(StatusCommand),
        Box::new(VersionCommand),
        Box::new(NewSessionCommand),
        Box::new(QuitCommand),
    ]
}
