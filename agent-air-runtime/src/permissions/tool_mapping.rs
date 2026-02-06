//! Tool-to-permission mapping for the grant system.
//!
//! This module defines how tools map to permission requests in the new
//! grant-based permission system. It provides a clear interface for tools
//! to create properly typed permission requests.

use std::path::Path;

use super::{GrantTarget, PermissionLevel, PermissionRequest};

/// Tool categories for permission mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    /// File reading tools: read_file, glob, grep, ls
    FileRead,
    /// File writing tools: write_file, edit_file, multi_edit
    FileWrite,
    /// Command execution: bash
    CommandExec,
    /// Network access: web_search, web_fetch
    Network,
    /// User interaction: ask_user_questions
    UserInteraction,
    /// Permission management: ask_for_permissions
    PermissionManagement,
}

impl ToolCategory {
    /// Returns the default permission level for this tool category.
    pub fn default_level(&self) -> PermissionLevel {
        match self {
            ToolCategory::FileRead => PermissionLevel::Read,
            ToolCategory::FileWrite => PermissionLevel::Write,
            ToolCategory::CommandExec => PermissionLevel::Execute,
            ToolCategory::Network => PermissionLevel::Read,
            ToolCategory::UserInteraction => PermissionLevel::None,
            ToolCategory::PermissionManagement => PermissionLevel::None,
        }
    }

    /// Returns whether this category requires permission checks.
    pub fn requires_permission(&self) -> bool {
        match self {
            ToolCategory::FileRead => true,
            ToolCategory::FileWrite => true,
            ToolCategory::CommandExec => true,
            ToolCategory::Network => true,
            ToolCategory::UserInteraction => false,
            ToolCategory::PermissionManagement => false,
        }
    }
}

/// Helper functions for creating tool-specific permission requests.
pub struct ToolPermissions;

impl ToolPermissions {
    /// Creates a permission request for reading a file.
    pub fn file_read(tool_use_id: &str, path: impl AsRef<Path>) -> PermissionRequest {
        let path = path.as_ref();
        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(path, false),
            PermissionLevel::Read,
            format!("Read file: {}", path.display()),
        )
        .with_tool("read_file")
    }

    /// Creates a permission request for writing a file.
    pub fn file_write(
        tool_use_id: &str,
        path: impl AsRef<Path>,
        is_create: bool,
    ) -> PermissionRequest {
        let path = path.as_ref();
        let action = if is_create { "Create" } else { "Write" };
        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(path, false),
            PermissionLevel::Write,
            format!("{} file: {}", action, path.display()),
        )
        .with_tool("write_file")
    }

    /// Creates a permission request for editing a file.
    pub fn file_edit(tool_use_id: &str, path: impl AsRef<Path>) -> PermissionRequest {
        let path = path.as_ref();
        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(path, false),
            PermissionLevel::Write,
            format!("Edit file: {}", path.display()),
        )
        .with_tool("edit_file")
    }

    /// Creates a permission request for multi-edit operations.
    pub fn multi_edit(tool_use_id: &str, paths: &[impl AsRef<Path>]) -> Vec<PermissionRequest> {
        paths
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let path = path.as_ref();
                PermissionRequest::new(
                    format!("{}-{}", tool_use_id, i),
                    GrantTarget::path(path, false),
                    PermissionLevel::Write,
                    format!("Edit file: {}", path.display()),
                )
                .with_tool("multi_edit")
            })
            .collect()
    }

    /// Creates a permission request for glob/search in a directory.
    pub fn glob_search(tool_use_id: &str, directory: impl AsRef<Path>) -> PermissionRequest {
        let directory = directory.as_ref();
        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(directory, true),
            PermissionLevel::Read,
            format!("Search in: {}", directory.display()),
        )
        .with_tool("glob")
    }

    /// Creates a permission request for grep/search in a directory.
    pub fn grep_search(tool_use_id: &str, directory: impl AsRef<Path>) -> PermissionRequest {
        let directory = directory.as_ref();
        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(directory, true),
            PermissionLevel::Read,
            format!("Search content in: {}", directory.display()),
        )
        .with_tool("grep")
    }

    /// Creates a permission request for listing a directory.
    pub fn list_directory(tool_use_id: &str, directory: impl AsRef<Path>) -> PermissionRequest {
        let directory = directory.as_ref();
        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(directory, false),
            PermissionLevel::Read,
            format!("List directory: {}", directory.display()),
        )
        .with_tool("ls")
    }

    /// Creates a permission request for bash command execution.
    ///
    /// The permission level is determined by the command:
    /// - Read-only commands (ls, cat, git status) -> Execute
    /// - Modifying commands (git commit, cargo build) -> Execute
    /// - Dangerous commands (rm -rf, sudo) -> Admin
    pub fn bash_command(tool_use_id: &str, command: &str) -> PermissionRequest {
        let level = classify_bash_command(command);
        PermissionRequest::new(
            tool_use_id,
            GrantTarget::command(command),
            level,
            format!("Execute: {}", truncate_command(command, 60)),
        )
        .with_tool("bash")
    }

    /// Creates a permission request for network access.
    pub fn network_access(tool_use_id: &str, domain: &str, method: &str) -> PermissionRequest {
        let level = match method.to_uppercase().as_str() {
            "GET" | "HEAD" | "OPTIONS" => PermissionLevel::Read,
            "POST" | "PUT" | "PATCH" => PermissionLevel::Write,
            "DELETE" => PermissionLevel::Execute,
            _ => PermissionLevel::Execute,
        };
        PermissionRequest::new(
            tool_use_id,
            GrantTarget::domain(domain),
            level,
            format!("{} {}", method.to_uppercase(), domain),
        )
        .with_tool("web_fetch")
    }

    /// Creates a permission request for web search.
    pub fn web_search(tool_use_id: &str, query: &str) -> PermissionRequest {
        PermissionRequest::new(
            tool_use_id,
            GrantTarget::domain("*"),
            PermissionLevel::Read,
            format!("Web search: {}", truncate_command(query, 40)),
        )
        .with_tool("web_search")
    }
}

/// Classifies a bash command to determine required permission level.
fn classify_bash_command(command: &str) -> PermissionLevel {
    let command_lower = command.to_lowercase();
    let first_word = command_lower.split_whitespace().next().unwrap_or("");

    // Dangerous commands requiring Admin
    let dangerous_patterns = [
        "rm -rf",
        "rm -fr",
        "sudo",
        "chmod -R",
        "chown -R",
        "mkfs",
        "dd if=",
        ":(){ :|:& };:",
        "> /dev/",
        "shutdown",
        "reboot",
        "init ",
        "systemctl",
    ];

    for pattern in dangerous_patterns {
        if command_lower.contains(pattern) {
            return PermissionLevel::Admin;
        }
    }

    // Commands that delete files
    if first_word == "rm" || command_lower.contains("--delete") {
        return PermissionLevel::Admin;
    }

    // Read-only commands
    let readonly_commands = [
        "ls",
        "cat",
        "head",
        "tail",
        "less",
        "more",
        "pwd",
        "whoami",
        "echo",
        "printf",
        "date",
        "which",
        "whereis",
        "file",
        "stat",
        "wc",
        "grep",
        "find",
        "locate",
        "tree",
        "df",
        "du",
        "git status",
        "git log",
        "git diff",
        "git show",
        "git branch",
    ];

    for readonly in readonly_commands {
        if command_lower.starts_with(readonly) {
            return PermissionLevel::Read;
        }
    }

    // Most commands need Execute
    PermissionLevel::Execute
}

/// Truncates a command for display purposes.
fn truncate_command(command: &str, max_len: usize) -> String {
    if command.len() <= max_len {
        command.to_string()
    } else {
        format!("{}...", &command[..max_len - 3])
    }
}

/// Gets the tool category from a tool name.
pub fn get_tool_category(tool_name: &str) -> ToolCategory {
    match tool_name {
        "read_file" | "glob" | "grep" | "ls" => ToolCategory::FileRead,
        "write_file" | "edit_file" | "multi_edit" => ToolCategory::FileWrite,
        "bash" => ToolCategory::CommandExec,
        "web_search" | "web_fetch" => ToolCategory::Network,
        "ask_user_questions" => ToolCategory::UserInteraction,
        "ask_for_permissions" => ToolCategory::PermissionManagement,
        _ => ToolCategory::FileRead, // Default to most restrictive
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_read_request() {
        let request = ToolPermissions::file_read("tool-1", "/project/src/main.rs");
        assert_eq!(request.required_level, PermissionLevel::Read);
        assert_eq!(request.tool_name, Some("read_file".to_string()));
    }

    #[test]
    fn test_file_write_request() {
        let request = ToolPermissions::file_write("tool-1", "/project/new_file.rs", true);
        assert_eq!(request.required_level, PermissionLevel::Write);
        assert!(request.description.contains("Create"));
    }

    #[test]
    fn test_bash_command_readonly() {
        let request = ToolPermissions::bash_command("tool-1", "ls -la");
        assert_eq!(request.required_level, PermissionLevel::Read);
    }

    #[test]
    fn test_bash_command_execute() {
        let request = ToolPermissions::bash_command("tool-1", "cargo build");
        assert_eq!(request.required_level, PermissionLevel::Execute);
    }

    #[test]
    fn test_bash_command_admin() {
        let request = ToolPermissions::bash_command("tool-1", "sudo apt install foo");
        assert_eq!(request.required_level, PermissionLevel::Admin);

        let request2 = ToolPermissions::bash_command("tool-1", "rm -rf /tmp/foo");
        assert_eq!(request2.required_level, PermissionLevel::Admin);
    }

    #[test]
    fn test_network_access() {
        let get_request = ToolPermissions::network_access("tool-1", "api.github.com", "GET");
        assert_eq!(get_request.required_level, PermissionLevel::Read);

        let post_request = ToolPermissions::network_access("tool-1", "api.github.com", "POST");
        assert_eq!(post_request.required_level, PermissionLevel::Write);

        let delete_request = ToolPermissions::network_access("tool-1", "api.github.com", "DELETE");
        assert_eq!(delete_request.required_level, PermissionLevel::Execute);
    }

    #[test]
    fn test_multi_edit() {
        let paths = vec!["/file1.rs", "/file2.rs"];
        let requests = ToolPermissions::multi_edit("tool-1", &paths);
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].id, "tool-1-0");
        assert_eq!(requests[1].id, "tool-1-1");
    }

    #[test]
    fn test_tool_category() {
        assert_eq!(get_tool_category("read_file"), ToolCategory::FileRead);
        assert_eq!(get_tool_category("write_file"), ToolCategory::FileWrite);
        assert_eq!(get_tool_category("bash"), ToolCategory::CommandExec);
        assert_eq!(get_tool_category("web_search"), ToolCategory::Network);
    }

    #[test]
    fn test_category_requires_permission() {
        assert!(ToolCategory::FileRead.requires_permission());
        assert!(ToolCategory::FileWrite.requires_permission());
        assert!(ToolCategory::CommandExec.requires_permission());
        assert!(!ToolCategory::UserInteraction.requires_permission());
    }
}
