//! Glob tool implementation for fast file pattern matching.
//!
//! This tool allows the LLM to find files matching glob patterns.
//! It integrates with the PermissionRegistry to require user approval
//! before searching directories.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use globset::{Glob, GlobMatcher};
use walkdir::WalkDir;

use super::types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};
use crate::permissions::{GrantTarget, PermissionLevel, PermissionRegistry, PermissionRequest};

/// Glob tool name constant.
pub const GLOB_TOOL_NAME: &str = "glob";

/// Glob tool description constant.
pub const GLOB_TOOL_DESCRIPTION: &str = r#"Fast file pattern matching tool that works with any codebase size.

Usage:
- Supports glob patterns like "**/*.js" or "src/**/*.ts"
- Returns matching file paths sorted by modification time (most recent first)
- Use this tool when you need to find files by name patterns
- The path parameter specifies the directory to search in (defaults to current directory)

Pattern Syntax:
- `*` matches any sequence of characters except path separators
- `**` matches any sequence of characters including path separators (recursive)
- `?` matches any single character except path separators
- `[abc]` matches any character in the brackets
- `[!abc]` matches any character not in the brackets
- `{a,b}` matches either pattern a or pattern b

Examples:
- "*.rs" - all Rust files in the search directory
- "**/*.rs" - all Rust files recursively
- "src/**/*.{ts,tsx}" - all TypeScript files under src/
- "**/test_*.py" - all Python test files recursively"#;

/// Glob tool JSON schema constant.
pub const GLOB_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "pattern": {
            "type": "string",
            "description": "The glob pattern to match files against"
        },
        "path": {
            "type": "string",
            "description": "The directory to search in. Defaults to current working directory."
        },
        "limit": {
            "type": "integer",
            "description": "Maximum number of results to return. Defaults to 1000."
        },
        "include_hidden": {
            "type": "boolean",
            "description": "Include hidden files (starting with dot). Defaults to false."
        }
    },
    "required": ["pattern"]
}"#;

/// Tool that finds files matching glob patterns with permission checks.
pub struct GlobTool {
    /// Reference to the permission registry for requesting read permissions.
    permission_registry: Arc<PermissionRegistry>,
    /// Default search path if none provided.
    default_path: Option<PathBuf>,
}

impl GlobTool {
    /// Create a new GlobTool with the given permission registry.
    ///
    /// # Arguments
    /// * `permission_registry` - The registry used to request and cache permissions.
    pub fn new(permission_registry: Arc<PermissionRegistry>) -> Self {
        Self {
            permission_registry,
            default_path: None,
        }
    }

    /// Create a new GlobTool with a default search path.
    ///
    /// # Arguments
    /// * `permission_registry` - The registry used to request and cache permissions.
    /// * `default_path` - The default directory to search if no path is provided.
    pub fn with_default_path(
        permission_registry: Arc<PermissionRegistry>,
        default_path: PathBuf,
    ) -> Self {
        Self {
            permission_registry,
            default_path: Some(default_path),
        }
    }

    /// Builds a permission request for searching files in a directory.
    fn build_permission_request(tool_use_id: &str, path: &str, pattern: &str) -> PermissionRequest {
        let reason = format!("Search for '{}' pattern", pattern);
        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(path, true), // recursive for glob
            PermissionLevel::Read,
            format!("Glob search in: {}", path),
        )
        .with_reason(reason)
        .with_tool(GLOB_TOOL_NAME)
    }

    /// Get file modification time for sorting.
    fn get_mtime(path: &Path) -> SystemTime {
        path.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    }
}

impl Executable for GlobTool {
    fn name(&self) -> &str {
        GLOB_TOOL_NAME
    }

    fn description(&self) -> &str {
        GLOB_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        GLOB_TOOL_SCHEMA
    }

    fn tool_type(&self) -> ToolType {
        ToolType::FileRead
    }

    fn execute(
        &self,
        context: ToolContext,
        input: HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let permission_registry = self.permission_registry.clone();
        let default_path = self.default_path.clone();

        Box::pin(async move {
            // ─────────────────────────────────────────────────────────────
            // Step 1: Extract and validate parameters
            // ─────────────────────────────────────────────────────────────
            let pattern = input
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'pattern' parameter".to_string())?;

            let search_path = input
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .or(default_path)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

            let search_path_str = search_path.to_string_lossy().to_string();

            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .map(|v| v.max(1) as usize)
                .unwrap_or(1000);

            let include_hidden = input
                .get("include_hidden")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            // Validate search path exists
            if !search_path.exists() {
                return Err(format!("Search path does not exist: {}", search_path_str));
            }

            if !search_path.is_dir() {
                return Err(format!(
                    "Search path is not a directory: {}",
                    search_path_str
                ));
            }

            // ─────────────────────────────────────────────────────────────
            // Step 2: Request permission if not pre-approved by batch executor
            // ─────────────────────────────────────────────────────────────
            if !context.permissions_pre_approved {
                let permission_request =
                    Self::build_permission_request(&context.tool_use_id, &search_path_str, pattern);

                let response_rx = permission_registry
                    .request_permission(
                        context.session_id,
                        permission_request,
                        context.turn_id.clone(),
                    )
                    .await
                    .map_err(|e| format!("Failed to request permission: {}", e))?;

                let response = response_rx
                    .await
                    .map_err(|_| "Permission request was cancelled".to_string())?;

                if !response.granted {
                    let reason = response
                        .message
                        .unwrap_or_else(|| "Permission denied by user".to_string());
                    return Err(format!(
                        "Permission denied to search '{}': {}",
                        search_path_str, reason
                    ));
                }
            }

            // ─────────────────────────────────────────────────────────────
            // Step 3: Compile glob pattern
            // ─────────────────────────────────────────────────────────────
            let glob_matcher: GlobMatcher = Glob::new(pattern)
                .map_err(|e| format!("Invalid glob pattern '{}': {}", pattern, e))?
                .compile_matcher();

            // ─────────────────────────────────────────────────────────────
            // Step 8: Walk directory and collect matches
            // ─────────────────────────────────────────────────────────────
            let search_path_for_filter = search_path.clone();
            let mut matches: Vec<PathBuf> = WalkDir::new(&search_path)
                .follow_links(false)
                .into_iter()
                .filter_entry(move |e| {
                    // Skip hidden directories unless include_hidden is true
                    // But always allow the root search path
                    let is_root = e.path() == search_path_for_filter;
                    is_root || include_hidden || !e.file_name().to_string_lossy().starts_with('.')
                })
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .filter(|e| {
                    let path = e.path();
                    let relative = path.strip_prefix(&search_path).unwrap_or(path);
                    glob_matcher.is_match(relative)
                })
                .map(|e| e.path().to_path_buf())
                .collect();

            // ─────────────────────────────────────────────────────────────
            // Step 9: Sort by modification time (most recent first)
            // ─────────────────────────────────────────────────────────────
            matches.sort_by(|a, b| Self::get_mtime(b).cmp(&Self::get_mtime(a)));

            // ─────────────────────────────────────────────────────────────
            // Step 10: Apply limit and format output
            // ─────────────────────────────────────────────────────────────
            let total_matches = matches.len();

            if matches.is_empty() {
                return Ok(format!(
                    "No files found matching pattern '{}' in '{}'",
                    pattern, search_path_str
                ));
            }

            matches.truncate(limit);

            let mut output = String::new();
            for path in &matches {
                output.push_str(&path.display().to_string());
                output.push('\n');
            }

            if total_matches > limit {
                output.push_str(&format!(
                    "\n... and {} more files (showing {}/{})",
                    total_matches - limit,
                    limit,
                    total_matches
                ));
            }

            Ok(output.trim_end().to_string())
        })
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Glob".to_string(),
            display_title: Box::new(|input| {
                input
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*")
                    .to_string()
            }),
            display_content: Box::new(|_input, result| {
                let lines: Vec<&str> = result.lines().take(20).collect();
                let total_lines = result.lines().count();

                DisplayResult {
                    content: lines.join("\n"),
                    content_type: ResultContentType::PlainText,
                    is_truncated: total_lines > 20,
                    full_length: total_lines,
                }
            }),
        }
    }

    fn compact_summary(&self, input: &HashMap<String, serde_json::Value>, result: &str) -> String {
        let pattern = input.get("pattern").and_then(|v| v.as_str()).unwrap_or("*");

        let file_count = result
            .lines()
            .filter(|line| {
                !line.starts_with("...") && !line.starts_with("No files") && !line.is_empty()
            })
            .count();

        format!("[Glob: {} ({} files)]", pattern, file_count)
    }

    fn required_permissions(
        &self,
        context: &ToolContext,
        input: &HashMap<String, serde_json::Value>,
    ) -> Option<Vec<PermissionRequest>> {
        // Extract pattern - required parameter
        let pattern = input.get("pattern").and_then(|v| v.as_str())?;

        // Extract path or use default_path, mirroring execute() logic
        let search_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .or_else(|| self.default_path.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let search_path_str = search_path.to_string_lossy().to_string();

        // Build permission request using the helper method
        let permission_request =
            Self::build_permission_request(&context.tool_use_id, &search_path_str, pattern);

        Some(vec![permission_request])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::types::ControllerEvent;
    use crate::permissions::{PermissionLevel, PermissionPanelResponse};
    use std::fs;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    /// Helper to create a permission registry for testing.
    fn create_test_registry() -> (Arc<PermissionRegistry>, mpsc::Receiver<ControllerEvent>) {
        let (tx, rx) = mpsc::channel(16);
        let registry = Arc::new(PermissionRegistry::new(tx));
        (registry, rx)
    }

    fn grant_once() -> PermissionPanelResponse {
        PermissionPanelResponse {
            granted: true,
            grant: None,
            message: None,
        }
    }

    fn deny(reason: &str) -> PermissionPanelResponse {
        PermissionPanelResponse {
            granted: false,
            grant: None,
            message: Some(reason.to_string()),
        }
    }

    fn setup_test_dir() -> TempDir {
        let temp = TempDir::new().unwrap();

        // Create test structure
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::create_dir_all(temp.path().join("tests")).unwrap();
        fs::create_dir_all(temp.path().join(".hidden_dir")).unwrap();
        fs::write(temp.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(temp.path().join("src/lib.rs"), "pub mod lib;").unwrap();
        fs::write(temp.path().join("tests/test.rs"), "#[test]").unwrap();
        fs::write(temp.path().join("README.md"), "# README").unwrap();
        fs::write(temp.path().join(".hidden"), "hidden file").unwrap();
        fs::write(temp.path().join(".hidden_dir/secret.txt"), "secret").unwrap();

        temp
    }

    #[tokio::test]
    async fn test_simple_pattern_with_permission() {
        let temp = setup_test_dir();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GlobTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("*.md".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-glob-1".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        // Grant permission in background
        let registry_clone = registry.clone();
        tokio::spawn(async move {
            if let Some(ControllerEvent::PermissionRequired { tool_use_id, .. }) =
                event_rx.recv().await
            {
                registry_clone
                    .respond_to_request(&tool_use_id, grant_once())
                    .await
                    .unwrap();
            }
        });

        let result = tool.execute(context, input).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("README.md"));
    }

    #[tokio::test]
    async fn test_recursive_pattern() {
        let temp = setup_test_dir();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GlobTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("**/*.rs".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-glob-recursive".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let registry_clone = registry.clone();
        tokio::spawn(async move {
            if let Some(ControllerEvent::PermissionRequired { tool_use_id, .. }) =
                event_rx.recv().await
            {
                registry_clone
                    .respond_to_request(&tool_use_id, grant_once())
                    .await
                    .unwrap();
            }
        });

        let result = tool.execute(context, input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("main.rs"));
        assert!(output.contains("lib.rs"));
        assert!(output.contains("test.rs"));
    }

    #[tokio::test]
    async fn test_hidden_files_excluded() {
        let temp = setup_test_dir();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GlobTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("**/*".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-glob-hidden".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let registry_clone = registry.clone();
        tokio::spawn(async move {
            if let Some(ControllerEvent::PermissionRequired { tool_use_id, .. }) =
                event_rx.recv().await
            {
                registry_clone
                    .respond_to_request(&tool_use_id, grant_once())
                    .await
                    .unwrap();
            }
        });

        let result = tool.execute(context, input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        // Should not contain hidden files
        assert!(!output.contains(".hidden"));
        assert!(!output.contains("secret.txt"));
    }

    #[tokio::test]
    async fn test_hidden_files_included() {
        let temp = setup_test_dir();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GlobTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("**/*".to_string()),
        );
        input.insert("include_hidden".to_string(), serde_json::Value::Bool(true));

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-glob-hidden-incl".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let registry_clone = registry.clone();
        tokio::spawn(async move {
            if let Some(ControllerEvent::PermissionRequired { tool_use_id, .. }) =
                event_rx.recv().await
            {
                registry_clone
                    .respond_to_request(&tool_use_id, grant_once())
                    .await
                    .unwrap();
            }
        });

        let result = tool.execute(context, input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        // Should contain hidden files when include_hidden is true
        assert!(output.contains(".hidden") || output.contains("secret.txt"));
    }

    #[tokio::test]
    async fn test_permission_denied() {
        let temp = setup_test_dir();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GlobTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("*.rs".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-glob-denied".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let registry_clone = registry.clone();
        tokio::spawn(async move {
            if let Some(ControllerEvent::PermissionRequired { tool_use_id, .. }) =
                event_rx.recv().await
            {
                registry_clone
                    .respond_to_request(&tool_use_id, deny("Access denied"))
                    .await
                    .unwrap();
            }
        });

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Permission denied"));
    }

    #[tokio::test]
    async fn test_limit() {
        let temp = setup_test_dir();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GlobTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("**/*".to_string()),
        );
        input.insert("limit".to_string(), serde_json::Value::Number(2.into()));

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-glob-limit".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let registry_clone = registry.clone();
        tokio::spawn(async move {
            if let Some(ControllerEvent::PermissionRequired { tool_use_id, .. }) =
                event_rx.recv().await
            {
                registry_clone
                    .respond_to_request(&tool_use_id, grant_once())
                    .await
                    .unwrap();
            }
        });

        let result = tool.execute(context, input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        // Should indicate more files available
        assert!(output.contains("... and"));
    }

    #[tokio::test]
    async fn test_no_matches() {
        let temp = setup_test_dir();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GlobTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("*.nonexistent".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-glob-nomatch".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let registry_clone = registry.clone();
        tokio::spawn(async move {
            if let Some(ControllerEvent::PermissionRequired { tool_use_id, .. }) =
                event_rx.recv().await
            {
                registry_clone
                    .respond_to_request(&tool_use_id, grant_once())
                    .await
                    .unwrap();
            }
        });

        let result = tool.execute(context, input).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No files found"));
    }

    #[tokio::test]
    async fn test_invalid_pattern() {
        let temp = setup_test_dir();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GlobTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        // Invalid glob pattern (unclosed bracket)
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("[invalid".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-glob-invalid".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let registry_clone = registry.clone();
        tokio::spawn(async move {
            if let Some(ControllerEvent::PermissionRequired { tool_use_id, .. }) =
                event_rx.recv().await
            {
                registry_clone
                    .respond_to_request(&tool_use_id, grant_once())
                    .await
                    .unwrap();
            }
        });

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid glob pattern"));
    }

    #[tokio::test]
    async fn test_missing_pattern() {
        let (registry, _event_rx) = create_test_registry();
        let tool = GlobTool::new(registry);

        let input = HashMap::new();

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required 'pattern'"));
    }

    #[tokio::test]
    async fn test_nonexistent_path() {
        let (registry, _event_rx) = create_test_registry();
        let tool = GlobTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("*.rs".to_string()),
        );
        input.insert(
            "path".to_string(),
            serde_json::Value::String("/nonexistent/path".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_compact_summary() {
        let (registry, _event_rx) = create_test_registry();
        let tool = GlobTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("**/*.rs".to_string()),
        );

        let result = "/path/main.rs\n/path/lib.rs\n/path/test.rs";
        let summary = tool.compact_summary(&input, result);
        assert_eq!(summary, "[Glob: **/*.rs (3 files)]");
    }

    #[test]
    fn test_compact_summary_no_matches() {
        let (registry, _event_rx) = create_test_registry();
        let tool = GlobTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("*.xyz".to_string()),
        );

        let result = "No files found matching pattern '*.xyz' in '/path'";
        let summary = tool.compact_summary(&input, result);
        assert_eq!(summary, "[Glob: *.xyz (0 files)]");
    }

    #[test]
    fn test_build_permission_request() {
        let request = GlobTool::build_permission_request("tool-123", "/path/to/src", "**/*.rs");

        assert_eq!(request.description, "Glob search in: /path/to/src");
        assert_eq!(
            request.reason,
            Some("Search for '**/*.rs' pattern".to_string())
        );
        assert_eq!(request.target, GrantTarget::path("/path/to/src", true));
        assert_eq!(request.required_level, PermissionLevel::Read);
    }
}
