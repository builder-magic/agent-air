//! Helper functions for slash command parsing and filtering.

use super::traits::SlashCommand;

/// Check if input starts with a slash command.
///
/// # Example
///
/// ```ignore
/// assert!(is_slash_command("/help"));
/// assert!(!is_slash_command("hello"));
/// ```
pub fn is_slash_command(input: &str) -> bool {
    input.starts_with('/')
}

/// Parse slash command from input, returning (command_name, args).
///
/// # Example
///
/// ```ignore
/// assert_eq!(parse_command("/help"), Some(("help", "")));
/// assert_eq!(parse_command("/echo hello"), Some(("echo", "hello")));
/// assert_eq!(parse_command("not a command"), None);
/// ```
pub fn parse_command(input: &str) -> Option<(&str, &str)> {
    if !is_slash_command(input) {
        return None;
    }

    let trimmed = input.trim_start_matches('/');
    let mut parts = trimmed.splitn(2, ' ');
    let name = parts.next()?;
    let args = parts.next().unwrap_or("").trim();

    Some((name, args))
}

/// Filter commands that match the given input prefix.
///
/// Input can include or omit the leading slash.
///
/// # Example
///
/// ```ignore
/// let matches = filter_commands(&commands, "/he");
/// // Returns commands starting with "he" (e.g., "help")
/// ```
pub fn filter_commands<'a>(
    commands: &'a [Box<dyn SlashCommand>],
    input: &str,
) -> Vec<&'a dyn SlashCommand> {
    let search_term = input.trim_start_matches('/').to_lowercase();

    commands
        .iter()
        .filter(|cmd| cmd.name().to_lowercase().starts_with(&search_term))
        .map(|c| c.as_ref())
        .collect()
}

/// Get a command by its exact name.
///
/// Name can include or omit the leading slash.
pub fn get_command_by_name<'a>(
    commands: &'a [Box<dyn SlashCommand>],
    name: &str,
) -> Option<&'a dyn SlashCommand> {
    let name = name.trim_start_matches('/');
    commands
        .iter()
        .find(|cmd| cmd.name() == name)
        .map(|c| c.as_ref())
}

/// Generate help message listing all available commands.
pub fn generate_help_message(commands: &[Box<dyn SlashCommand>]) -> String {
    let mut help = String::from("Available commands:\n\n");

    for cmd in commands {
        help.push_str(&format!("  /{} - {}\n", cmd.name(), cmd.description()));
    }

    help
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_slash_command() {
        assert!(is_slash_command("/help"));
        assert!(is_slash_command("/"));
        assert!(!is_slash_command("help"));
        assert!(!is_slash_command(""));
    }

    #[test]
    fn test_parse_command() {
        assert_eq!(parse_command("/help"), Some(("help", "")));
        assert_eq!(
            parse_command("/echo hello world"),
            Some(("echo", "hello world"))
        );
        assert_eq!(
            parse_command("/cmd  spaced  args"),
            Some(("cmd", "spaced  args"))
        );
        assert_eq!(parse_command("not a command"), None);
        assert_eq!(parse_command(""), None);
    }
}
