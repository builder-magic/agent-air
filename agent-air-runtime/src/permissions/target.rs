//! Grant targets for the permission system.
//!
//! Targets define what a permission grant applies to:
//! - `Path`: File system paths (files and directories)
//! - `Domain`: Network domains for HTTP access
//! - `Command`: Shell command patterns
//! - `Tool`: Named tool invocations

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The target of a permission grant.
///
/// Each target type has its own matching semantics:
/// - `Path`: Matches file system paths with optional recursion
/// - `Domain`: Matches network domains with wildcard support
/// - `Command`: Matches shell commands with glob patterns
/// - `Tool`: Matches tool names exactly
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GrantTarget {
    /// File or directory path target.
    Path {
        /// The directory or file path this grant applies to.
        path: PathBuf,
        /// Whether the grant applies to subdirectories recursively.
        recursive: bool,
    },
    /// Network domain target for HTTP requests.
    Domain {
        /// Domain pattern (e.g., "api.github.com", "*.anthropic.com", "*").
        pattern: String,
    },
    /// Shell command pattern target.
    Command {
        /// Command pattern (e.g., "git *", "cargo build", "*").
        pattern: String,
    },
    /// Tool invocation target.
    Tool {
        /// Tool name (e.g., "switch_aws_account", "delete_resource").
        tool_name: String,
    },
}

impl GrantTarget {
    /// Creates a new path target.
    ///
    /// # Arguments
    /// * `path` - The directory or file path
    /// * `recursive` - Whether to apply to subdirectories
    pub fn path(path: impl Into<PathBuf>, recursive: bool) -> Self {
        Self::Path {
            path: path.into(),
            recursive,
        }
    }

    /// Creates a new domain target.
    ///
    /// # Arguments
    /// * `pattern` - Domain pattern (e.g., "api.github.com", "*.example.com")
    pub fn domain(pattern: impl Into<String>) -> Self {
        Self::Domain {
            pattern: pattern.into(),
        }
    }

    /// Creates a new command target.
    ///
    /// # Arguments
    /// * `pattern` - Command pattern (e.g., "git *", "npm install")
    pub fn command(pattern: impl Into<String>) -> Self {
        Self::Command {
            pattern: pattern.into(),
        }
    }

    /// Creates a new tool target.
    ///
    /// # Arguments
    /// * `tool_name` - Tool name (e.g., "switch_aws_account", "delete_resource")
    pub fn tool(tool_name: impl Into<String>) -> Self {
        Self::Tool {
            tool_name: tool_name.into(),
        }
    }

    /// Checks if this grant target covers a request target.
    ///
    /// Coverage rules:
    /// - **Path**: Grant path must be ancestor of request path (if recursive) or parent/equal (if not)
    /// - **Domain**: Grant pattern must match request domain (wildcards supported)
    /// - **Command**: Grant pattern must match request command (glob matching)
    ///
    /// # Arguments
    /// * `request` - The target being requested
    ///
    /// # Returns
    /// `true` if this grant target covers the request target
    pub fn covers(&self, request: &GrantTarget) -> bool {
        match (self, request) {
            (
                GrantTarget::Path {
                    path: grant_path,
                    recursive,
                },
                GrantTarget::Path {
                    path: request_path, ..
                },
            ) => path_covers(grant_path, request_path, *recursive),

            (GrantTarget::Domain { pattern: grant }, GrantTarget::Domain { pattern: request }) => {
                domain_pattern_matches(grant, request)
            }

            (
                GrantTarget::Command { pattern: grant },
                GrantTarget::Command { pattern: request },
            ) => command_pattern_matches(grant, request),

            (
                GrantTarget::Tool {
                    tool_name: grant_name,
                },
                GrantTarget::Tool {
                    tool_name: request_name,
                },
            ) => grant_name == request_name,

            // Different target types never cover each other
            _ => false,
        }
    }

    /// Returns a display-friendly description of this target.
    pub fn description(&self) -> String {
        match self {
            GrantTarget::Path { path, recursive } => {
                if *recursive {
                    format!("{} (recursive)", path.display())
                } else {
                    format!("{}", path.display())
                }
            }
            GrantTarget::Domain { pattern } => pattern.clone(),
            GrantTarget::Command { pattern } => pattern.clone(),
            GrantTarget::Tool { tool_name } => tool_name.clone(),
        }
    }

    /// Returns the target type as a string for display purposes.
    pub fn target_type(&self) -> &'static str {
        match self {
            GrantTarget::Path { .. } => "Path",
            GrantTarget::Domain { .. } => "Domain",
            GrantTarget::Command { .. } => "Command",
            GrantTarget::Tool { .. } => "Tool",
        }
    }
}

/// Checks if a grant path covers a request path.
///
/// # Arguments
/// * `grant_path` - The path in the grant
/// * `request_path` - The path being requested
/// * `recursive` - Whether the grant is recursive
///
/// # Returns
/// `true` if the grant path covers the request path
fn path_covers(grant_path: &Path, request_path: &Path, recursive: bool) -> bool {
    // Normalize paths to handle . and .. components
    let normalized_grant = normalize_path(grant_path);
    let normalized_request = normalize_path(request_path);

    // Check for path traversal attacks (request escaping grant)
    if has_path_traversal(&normalized_request, &normalized_grant) {
        return false;
    }

    if recursive {
        // Recursive: request must be equal to or under grant path
        normalized_request.starts_with(&normalized_grant)
    } else {
        // Non-recursive: request must be exact match or direct child
        if normalized_request == normalized_grant {
            return true;
        }
        // Check if request is a direct child of grant path
        if let Some(parent) = normalized_request.parent() {
            parent == normalized_grant
        } else {
            false
        }
    }
}

/// Normalizes a path by resolving . and .. components.
///
/// Note: This does NOT resolve symlinks for security reasons.
/// Symlink resolution should be done separately before permission checks.
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {
                // Skip current directory markers
            }
            _ => {
                normalized.push(component);
            }
        }
    }

    normalized
}

/// Checks if a path attempts to traverse outside a base path.
fn has_path_traversal(request: &Path, grant: &Path) -> bool {
    // If request doesn't start with grant after normalization,
    // it might be attempting to escape via symlinks or other tricks
    // This is a basic check - full symlink resolution should be done separately
    !request.starts_with(grant) && request.parent() != Some(grant)
}

/// Checks if a domain pattern matches a domain.
///
/// # Pattern Rules
/// - Exact match: "api.github.com" matches "api.github.com"
/// - Wildcard prefix: "*.github.com" matches "api.github.com", "raw.github.com"
/// - Full wildcard: "*" matches any domain
fn domain_pattern_matches(pattern: &str, domain: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern == domain {
        return true;
    }

    if let Some(suffix) = pattern.strip_prefix("*.") {
        // Pattern is *.example.com, check if domain ends with .example.com
        // or is exactly example.com
        if domain == suffix {
            return true;
        }
        if domain.ends_with(&format!(".{}", suffix)) {
            return true;
        }
    }

    false
}

/// Checks if a command pattern matches a command.
///
/// # Pattern Rules
/// - Exact match: "git status" matches "git status"
/// - Wildcard suffix: "git *" matches "git status", "git commit -m 'msg'"
/// - Full wildcard: "*" matches any command
fn command_pattern_matches(pattern: &str, command: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern == command {
        return true;
    }

    // Handle "cmd *" pattern - matches "cmd" followed by anything
    if let Some(prefix) = pattern.strip_suffix(" *") {
        // Command must start with the prefix followed by a space or be exactly the prefix
        if command == prefix {
            return true;
        }
        if command.starts_with(&format!("{} ", prefix)) {
            return true;
        }
    }

    false
}

impl std::fmt::Display for GrantTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod path_tests {
        use super::*;

        #[test]
        fn test_exact_path_match() {
            let grant = GrantTarget::path("/project/src", false);
            let request = GrantTarget::path("/project/src", false);
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_direct_child_non_recursive() {
            let grant = GrantTarget::path("/project/src", false);
            let request = GrantTarget::path("/project/src/main.rs", false);
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_nested_child_non_recursive_fails() {
            let grant = GrantTarget::path("/project/src", false);
            let request = GrantTarget::path("/project/src/utils/mod.rs", false);
            assert!(!grant.covers(&request));
        }

        #[test]
        fn test_recursive_covers_nested() {
            let grant = GrantTarget::path("/project/src", true);
            let request = GrantTarget::path("/project/src/utils/mod.rs", false);
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_recursive_covers_deep_nested() {
            let grant = GrantTarget::path("/project", true);
            let request = GrantTarget::path("/project/src/utils/helpers/mod.rs", false);
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_sibling_path_not_covered() {
            let grant = GrantTarget::path("/project/src", true);
            let request = GrantTarget::path("/project/tests/test.rs", false);
            assert!(!grant.covers(&request));
        }

        #[test]
        fn test_parent_path_not_covered() {
            let grant = GrantTarget::path("/project/src", true);
            let request = GrantTarget::path("/project/Cargo.toml", false);
            assert!(!grant.covers(&request));
        }

        #[test]
        fn test_path_traversal_blocked() {
            let grant = GrantTarget::path("/project/src", true);
            let request = GrantTarget::path("/project/src/../secrets/key.pem", false);
            assert!(!grant.covers(&request));
        }

        #[test]
        fn test_unrelated_path_not_covered() {
            let grant = GrantTarget::path("/project", true);
            let request = GrantTarget::path("/etc/passwd", false);
            assert!(!grant.covers(&request));
        }

        #[test]
        fn test_path_prefix_collision_not_covered() {
            // CR-001: Verify that /project/src does NOT cover /project/src-backup
            // This tests that path matching uses component boundaries, not string prefixes
            let grant = GrantTarget::path("/project/src", true);

            // These should NOT be covered - they share a string prefix but are different directories
            let request1 = GrantTarget::path("/project/src-backup/file.rs", false);
            assert!(
                !grant.covers(&request1),
                "/project/src should not cover /project/src-backup"
            );

            let request2 = GrantTarget::path("/project/srcrc/file.rs", false);
            assert!(
                !grant.covers(&request2),
                "/project/src should not cover /project/srcrc"
            );

            let request3 = GrantTarget::path("/project/src_old/file.rs", false);
            assert!(
                !grant.covers(&request3),
                "/project/src should not cover /project/src_old"
            );

            // This SHOULD be covered - it's actually under /project/src
            let request4 = GrantTarget::path("/project/src/backup/file.rs", false);
            assert!(
                grant.covers(&request4),
                "/project/src should cover /project/src/backup"
            );
        }
    }

    mod domain_tests {
        use super::*;

        #[test]
        fn test_exact_domain_match() {
            let grant = GrantTarget::domain("api.github.com");
            let request = GrantTarget::domain("api.github.com");
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_wildcard_subdomain() {
            let grant = GrantTarget::domain("*.github.com");
            let request = GrantTarget::domain("api.github.com");
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_wildcard_matches_base_domain() {
            let grant = GrantTarget::domain("*.github.com");
            let request = GrantTarget::domain("github.com");
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_wildcard_all() {
            let grant = GrantTarget::domain("*");
            let request = GrantTarget::domain("any.domain.com");
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_different_domain_not_covered() {
            let grant = GrantTarget::domain("api.github.com");
            let request = GrantTarget::domain("api.gitlab.com");
            assert!(!grant.covers(&request));
        }

        #[test]
        fn test_wildcard_only_matches_direct_subdomains() {
            let grant = GrantTarget::domain("*.github.com");
            let request = GrantTarget::domain("evil.com");
            assert!(!grant.covers(&request));
        }
    }

    mod command_tests {
        use super::*;

        #[test]
        fn test_exact_command_match() {
            let grant = GrantTarget::command("git status");
            let request = GrantTarget::command("git status");
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_wildcard_command() {
            let grant = GrantTarget::command("git *");
            let request = GrantTarget::command("git status");
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_wildcard_command_with_args() {
            let grant = GrantTarget::command("git *");
            let request = GrantTarget::command("git commit -m 'message'");
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_wildcard_all_commands() {
            let grant = GrantTarget::command("*");
            let request = GrantTarget::command("rm -rf /");
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_different_command_not_covered() {
            let grant = GrantTarget::command("git *");
            let request = GrantTarget::command("docker run nginx");
            assert!(!grant.covers(&request));
        }

        #[test]
        fn test_partial_command_not_covered() {
            let grant = GrantTarget::command("git");
            let request = GrantTarget::command("git status");
            // "git" without wildcard only matches exactly "git"
            assert!(!grant.covers(&request));
        }

        #[test]
        fn test_wildcard_command_matches_bare_command() {
            let grant = GrantTarget::command("git *");
            let request = GrantTarget::command("git");
            assert!(grant.covers(&request));
        }
    }

    mod tool_tests {
        use super::*;

        #[test]
        fn test_exact_tool_match() {
            let grant = GrantTarget::tool("switch_aws_account");
            let request = GrantTarget::tool("switch_aws_account");
            assert!(grant.covers(&request));
        }

        #[test]
        fn test_different_tool_not_covered() {
            let grant = GrantTarget::tool("switch_aws_account");
            let request = GrantTarget::tool("delete_resource");
            assert!(!grant.covers(&request));
        }

        #[test]
        fn test_tool_description() {
            let target = GrantTarget::tool("switch_aws_account");
            assert_eq!(target.description(), "switch_aws_account");
        }

        #[test]
        fn test_tool_target_type() {
            let target = GrantTarget::tool("my_tool");
            assert_eq!(target.target_type(), "Tool");
        }
    }

    mod cross_target_tests {
        use super::*;

        #[test]
        fn test_different_target_types_dont_match() {
            let path_grant = GrantTarget::path("/project", true);
            let domain_request = GrantTarget::domain("github.com");
            assert!(!path_grant.covers(&domain_request));

            let command_grant = GrantTarget::command("git *");
            let path_request = GrantTarget::path("/project/src", false);
            assert!(!command_grant.covers(&path_request));

            let tool_grant = GrantTarget::tool("my_tool");
            let command_request = GrantTarget::command("my_tool");
            assert!(!tool_grant.covers(&command_request));
        }
    }

    mod serialization_tests {
        use super::*;

        #[test]
        fn test_path_serialization() {
            let target = GrantTarget::path("/project/src", true);
            let json = serde_json::to_string(&target).unwrap();
            assert!(json.contains("\"type\":\"path\""));
            assert!(json.contains("\"recursive\":true"));

            let deserialized: GrantTarget = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, target);
        }

        #[test]
        fn test_domain_serialization() {
            let target = GrantTarget::domain("*.github.com");
            let json = serde_json::to_string(&target).unwrap();
            assert!(json.contains("\"type\":\"domain\""));

            let deserialized: GrantTarget = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, target);
        }

        #[test]
        fn test_command_serialization() {
            let target = GrantTarget::command("git *");
            let json = serde_json::to_string(&target).unwrap();
            assert!(json.contains("\"type\":\"command\""));

            let deserialized: GrantTarget = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, target);
        }

        #[test]
        fn test_tool_serialization() {
            let target = GrantTarget::tool("switch_aws_account");
            let json = serde_json::to_string(&target).unwrap();
            assert!(json.contains("\"type\":\"tool\""));
            assert!(json.contains("\"tool_name\":\"switch_aws_account\""));

            let deserialized: GrantTarget = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, target);
        }
    }
}
