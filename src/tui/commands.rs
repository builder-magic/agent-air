// Slash command system for the TUI
//
// Provides default commands that work with any agent, plus support for
// agent-specific custom commands.

use super::widgets::SlashCommandTrait;

/// A slash command with its name and description
#[derive(Debug, Clone)]
pub struct SlashCommand {
    /// Command name without the slash (e.g., "help")
    pub name: &'static str,
    /// Brief description of what the command does
    pub description: &'static str,
}

impl SlashCommandTrait for SlashCommand {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }
}

impl SlashCommandTrait for &SlashCommand {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }
}

/// Default commands available in all agents
pub static DEFAULT_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "help",
        description: "Show available commands and usage",
    },
    SlashCommand {
        name: "clear",
        description: "Clear the conversation history",
    },
    SlashCommand {
        name: "status",
        description: "Show current session status",
    },
    SlashCommand {
        name: "new-session",
        description: "Create a new LLM session",
    },
    SlashCommand {
        name: "quit",
        description: "Exit the application",
    },
    SlashCommand {
        name: "version",
        description: "Show application version",
    },
    SlashCommand {
        name: "themes",
        description: "Open theme picker to change colors",
    },
    SlashCommand {
        name: "compact",
        description: "Compact the conversation history",
    },
    SlashCommand {
        name: "sessions",
        description: "View and switch between sessions",
    },
];

/// Returns all default slash commands
pub fn get_default_commands() -> &'static [SlashCommand] {
    DEFAULT_COMMANDS
}

/// Returns commands that match the given input prefix.
/// Input should include the leading slash (e.g., "/he" to match "/help")
pub fn filter_commands<'a>(
    commands: &'a [SlashCommand],
    input: &str,
) -> Vec<&'a SlashCommand> {
    let search_term = input.trim_start_matches('/');

    commands
        .iter()
        .filter(|cmd| cmd.name.starts_with(search_term))
        .collect()
}

/// Returns a command by its name, or None if not found
pub fn get_command_by_name<'a>(
    commands: &'a [SlashCommand],
    name: &str,
) -> Option<&'a SlashCommand> {
    let name = name.trim_start_matches('/');
    commands.iter().find(|cmd| cmd.name == name)
}

/// Check if input is a slash command
pub fn is_slash_command(input: &str) -> bool {
    input.starts_with('/')
}

/// Parse slash command from input, returning (command_name, args)
pub fn parse_command(input: &str) -> Option<(&str, &str)> {
    if !is_slash_command(input) {
        return None;
    }

    let trimmed = input.trim_start_matches('/');
    let mut parts = trimmed.splitn(2, ' ');
    let name = parts.next()?;
    let args = parts.next().unwrap_or("");

    Some((name, args))
}

/// Generate help message with all available commands
pub fn generate_help_message(commands: &[SlashCommand]) -> String {
    let mut help = String::from("Available commands:\n\n");

    for cmd in commands {
        help.push_str(&format!("/{} - {}\n", cmd.name, cmd.description));
    }

    help
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_commands() {
        let matches = filter_commands(DEFAULT_COMMANDS, "/he");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "help");

        let matches = filter_commands(DEFAULT_COMMANDS, "/");
        assert_eq!(matches.len(), DEFAULT_COMMANDS.len());
    }

    #[test]
    fn test_get_command_by_name() {
        assert!(get_command_by_name(DEFAULT_COMMANDS, "help").is_some());
        assert!(get_command_by_name(DEFAULT_COMMANDS, "/help").is_some());
        assert!(get_command_by_name(DEFAULT_COMMANDS, "unknown").is_none());
    }

    #[test]
    fn test_parse_command() {
        assert_eq!(parse_command("/help"), Some(("help", "")));
        assert_eq!(
            parse_command("/new-session arg"),
            Some(("new-session", "arg"))
        );
        assert_eq!(parse_command("not a command"), None);
    }

    #[test]
    fn test_is_slash_command() {
        assert!(is_slash_command("/help"));
        assert!(!is_slash_command("hello"));
    }
}
