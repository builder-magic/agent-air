//! Grep tool implementation using ripgrep for high-performance file searching.
//!
//! This tool allows the LLM to search file contents using regex patterns.
//! It integrates with the PermissionRegistry to require user approval
//! before performing read operations on directories.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use globset::{Glob, GlobMatcher};
use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};
use walkdir::WalkDir;

use super::types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};
use crate::permissions::{GrantTarget, PermissionLevel, PermissionRegistry, PermissionRequest};

/// Grep tool name constant.
pub const GREP_TOOL_NAME: &str = "grep";

/// Grep tool description constant.
pub const GREP_TOOL_DESCRIPTION: &str = r#"A powerful search tool built on ripgrep for searching file contents.

Usage:
- Supports full regex syntax (e.g., "log.*Error", "function\s+\w+")
- Filter files with glob parameter (e.g., "*.js", "**/*.tsx") or type parameter (e.g., "js", "py", "rust")
- Output modes: "content" shows matching lines, "files_with_matches" shows only file paths (default), "count" shows match counts
- Pattern syntax uses ripgrep - literal braces need escaping

Output Modes:
- "files_with_matches" (default): Returns only file paths that contain matches
- "content": Returns matching lines with optional context
- "count": Returns match count per file

Context Options (only with output_mode: "content"):
- -A: Lines after each match
- -B: Lines before each match
- -C: Lines before and after (context)
- -n: Show line numbers (default: true)

Examples:
- Search for "TODO" in all files: pattern="TODO"
- Search in Rust files only: pattern="impl.*Trait", type="rust"
- Search with context: pattern="error", output_mode="content", -C=3
- Case insensitive: pattern="error", -i=true"#;

/// Grep tool JSON schema constant.
pub const GREP_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "pattern": {
            "type": "string",
            "description": "The regular expression pattern to search for"
        },
        "path": {
            "type": "string",
            "description": "File or directory to search in. Defaults to current directory."
        },
        "glob": {
            "type": "string",
            "description": "Glob pattern to filter files (e.g., '*.js', '*.{ts,tsx}')"
        },
        "type": {
            "type": "string",
            "description": "File type to search (e.g., 'js', 'py', 'rust', 'go'). More efficient than glob for standard types."
        },
        "output_mode": {
            "type": "string",
            "enum": ["files_with_matches", "content", "count"],
            "description": "Output mode. Defaults to 'files_with_matches'."
        },
        "-i": {
            "type": "boolean",
            "description": "Case insensitive search. Defaults to false."
        },
        "-n": {
            "type": "boolean",
            "description": "Show line numbers (content mode only). Defaults to true."
        },
        "-A": {
            "type": "integer",
            "description": "Lines to show after each match (content mode only)."
        },
        "-B": {
            "type": "integer",
            "description": "Lines to show before each match (content mode only)."
        },
        "-C": {
            "type": "integer",
            "description": "Lines to show before and after each match (content mode only)."
        },
        "multiline": {
            "type": "boolean",
            "description": "Enable multiline mode where . matches newlines. Defaults to false."
        },
        "limit": {
            "type": "integer",
            "description": "Maximum number of results to return. Defaults to 1000."
        }
    },
    "required": ["pattern"]
}"#;

/// Output mode for grep results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Return only file paths that contain matches.
    FilesWithMatches,
    /// Return matching lines with optional context.
    Content,
    /// Return match count per file.
    Count,
}

impl OutputMode {
    fn from_str(s: &str) -> Self {
        match s {
            "content" => OutputMode::Content,
            "count" => OutputMode::Count,
            _ => OutputMode::FilesWithMatches,
        }
    }
}

/// Tool that searches file contents using ripgrep with permission checks.
pub struct GrepTool {
    /// Reference to the permission registry for requesting read permissions.
    permission_registry: Arc<PermissionRegistry>,
    /// Default search path if none provided.
    default_path: Option<PathBuf>,
}

impl GrepTool {
    /// Create a new GrepTool with the given permission registry.
    ///
    /// # Arguments
    /// * `permission_registry` - The registry used to request and cache permissions.
    pub fn new(permission_registry: Arc<PermissionRegistry>) -> Self {
        Self {
            permission_registry,
            default_path: None,
        }
    }

    /// Create a new GrepTool with a default search path.
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

    /// Builds a permission request for searching files in a path.
    fn build_permission_request(tool_use_id: &str, search_path: &str) -> PermissionRequest {
        let path = Path::new(search_path);
        let reason = "Search file contents using grep";

        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(path, true), // recursive for grep
            PermissionLevel::Read,
            &format!("Search files in: {}", path.display()),
        )
        .with_reason(reason)
        .with_tool(GREP_TOOL_NAME)
    }

    /// Get file type extensions for a given type name.
    fn get_type_extensions(file_type: &str) -> Vec<&'static str> {
        match file_type {
            "js" | "javascript" => vec!["js", "mjs", "cjs"],
            "ts" | "typescript" => vec!["ts", "mts", "cts"],
            "tsx" => vec!["tsx"],
            "jsx" => vec!["jsx"],
            "py" | "python" => vec!["py", "pyi"],
            "rust" | "rs" => vec!["rs"],
            "go" => vec!["go"],
            "java" => vec!["java"],
            "c" => vec!["c", "h"],
            "cpp" | "c++" => vec!["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
            "rb" | "ruby" => vec!["rb"],
            "php" => vec!["php"],
            "swift" => vec!["swift"],
            "kotlin" | "kt" => vec!["kt", "kts"],
            "scala" => vec!["scala"],
            "md" | "markdown" => vec!["md", "markdown"],
            "json" => vec!["json"],
            "yaml" | "yml" => vec!["yaml", "yml"],
            "toml" => vec!["toml"],
            "xml" => vec!["xml"],
            "html" => vec!["html", "htm"],
            "css" => vec!["css"],
            "sql" => vec!["sql"],
            "sh" | "bash" => vec!["sh", "bash"],
            _ => vec![],
        }
    }
}

impl Executable for GrepTool {
    fn name(&self) -> &str {
        GREP_TOOL_NAME
    }

    fn description(&self) -> &str {
        GREP_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        GREP_TOOL_SCHEMA
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

            // Verify path exists
            if !search_path.exists() {
                return Err(format!("Search path does not exist: {}", search_path_str));
            }

            let glob_pattern = input.get("glob").and_then(|v| v.as_str());
            let file_type = input.get("type").and_then(|v| v.as_str());

            let output_mode = input
                .get("output_mode")
                .and_then(|v| v.as_str())
                .map(OutputMode::from_str)
                .unwrap_or(OutputMode::FilesWithMatches);

            let case_insensitive = input.get("-i").and_then(|v| v.as_bool()).unwrap_or(false);

            let show_line_numbers = input.get("-n").and_then(|v| v.as_bool()).unwrap_or(true);

            let context_after = input
                .get("-A")
                .and_then(|v| v.as_i64())
                .map(|v| v.max(0) as usize)
                .unwrap_or(0);

            let context_before = input
                .get("-B")
                .and_then(|v| v.as_i64())
                .map(|v| v.max(0) as usize)
                .unwrap_or(0);

            let context_lines = input
                .get("-C")
                .and_then(|v| v.as_i64())
                .map(|v| v.max(0) as usize)
                .unwrap_or(0);

            // -C overrides -A and -B
            let (context_before, context_after) = if context_lines > 0 {
                (context_lines, context_lines)
            } else {
                (context_before, context_after)
            };

            let multiline = input
                .get("multiline")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .map(|v| v.max(1) as usize)
                .unwrap_or(1000);

            // ─────────────────────────────────────────────────────────────
            // Step 2: Request permission if not pre-approved by batch executor
            // ─────────────────────────────────────────────────────────────
            if !context.permissions_pre_approved {
                let permission_request =
                    Self::build_permission_request(&context.tool_use_id, &search_path_str);

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
            // Step 3: Build regex matcher
            // ─────────────────────────────────────────────────────────────
            let matcher = RegexMatcherBuilder::new()
                .case_insensitive(case_insensitive)
                .multi_line(multiline)
                .dot_matches_new_line(multiline)
                .build(pattern)
                .map_err(|e| format!("Invalid regex pattern: {}", e))?;

            // ─────────────────────────────────────────────────────────────
            // Step 8: Build file glob filter if provided
            // ─────────────────────────────────────────────────────────────
            let glob_matcher: Option<GlobMatcher> = if let Some(glob_str) = glob_pattern {
                Some(
                    Glob::new(glob_str)
                        .map_err(|e| format!("Invalid glob pattern: {}", e))?
                        .compile_matcher(),
                )
            } else {
                None
            };

            // Get file type extensions if provided
            let type_extensions: Option<Vec<&str>> =
                file_type.map(|t| Self::get_type_extensions(t));

            // ─────────────────────────────────────────────────────────────
            // Step 9: Build searcher with context options
            // ─────────────────────────────────────────────────────────────
            let mut searcher_builder = SearcherBuilder::new();
            searcher_builder
                .binary_detection(BinaryDetection::quit(0))
                .line_number(show_line_numbers);

            if context_before > 0 || context_after > 0 {
                searcher_builder
                    .before_context(context_before)
                    .after_context(context_after);
            }

            let mut searcher = searcher_builder.build();

            // ─────────────────────────────────────────────────────────────
            // Step 10: Collect files to search
            // ─────────────────────────────────────────────────────────────
            let files: Vec<PathBuf> = if search_path.is_file() {
                vec![search_path.clone()]
            } else {
                let search_path_clone = search_path.clone();
                WalkDir::new(&search_path)
                    .follow_links(false)
                    .into_iter()
                    .filter_entry(move |e| {
                        // Skip hidden directories, but not the root search path itself
                        let is_root = e.path() == search_path_clone;
                        is_root || !e.file_name().to_string_lossy().starts_with('.')
                    })
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .filter(|e| {
                        let path = e.path();

                        // Apply glob filter
                        if let Some(ref gm) = glob_matcher {
                            let relative = path.strip_prefix(&search_path).unwrap_or(path);
                            if !gm.is_match(relative) {
                                return false;
                            }
                        }

                        // Apply type filter
                        if let Some(ref exts) = type_extensions {
                            if exts.is_empty() {
                                // Unknown type, don't filter
                                return true;
                            }
                            if let Some(ext) = path.extension() {
                                let ext_str = ext.to_string_lossy().to_lowercase();
                                if !exts.iter().any(|e| *e == ext_str) {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        }

                        true
                    })
                    .map(|e| e.path().to_path_buf())
                    .collect()
            };

            // ─────────────────────────────────────────────────────────────
            // Step 11: Execute search based on output mode
            // ─────────────────────────────────────────────────────────────
            match output_mode {
                OutputMode::FilesWithMatches => {
                    search_files_with_matches(&mut searcher, &matcher, &files, limit)
                }
                OutputMode::Content => {
                    search_content(&mut searcher, &matcher, &files, show_line_numbers, limit)
                }
                OutputMode::Count => search_count(&mut searcher, &matcher, &files, limit),
            }
        })
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Grep".to_string(),
            display_title: Box::new(|input| {
                input
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .map(|p| {
                        if p.len() > 30 {
                            format!("{}...", &p[..30])
                        } else {
                            p.to_string()
                        }
                    })
                    .unwrap_or_default()
            }),
            display_content: Box::new(|_input, result| {
                let lines: Vec<&str> = result.lines().take(30).collect();
                let total_lines = result.lines().count();

                DisplayResult {
                    content: lines.join("\n"),
                    content_type: ResultContentType::PlainText,
                    is_truncated: total_lines > 30,
                    full_length: total_lines,
                }
            }),
        }
    }

    fn compact_summary(&self, input: &HashMap<String, serde_json::Value>, result: &str) -> String {
        let pattern = input
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|p| {
                if p.len() > 20 {
                    format!("{}...", &p[..20])
                } else {
                    p.to_string()
                }
            })
            .unwrap_or_else(|| "?".to_string());

        let match_count = result.lines().filter(|line| !line.is_empty()).count();

        format!("[Grep: '{}' ({} matches)]", pattern, match_count)
    }

    fn required_permissions(
        &self,
        context: &ToolContext,
        input: &HashMap<String, serde_json::Value>,
    ) -> Option<Vec<PermissionRequest>> {
        // Extract the path from input or use default_path
        let search_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .or_else(|| self.default_path.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let search_path_str = search_path.to_string_lossy().to_string();

        // Build the permission request using the existing helper method
        let permission_request =
            Self::build_permission_request(&context.tool_use_id, &search_path_str);

        Some(vec![permission_request])
    }
}

/// Search for files containing matches (files_with_matches mode).
fn search_files_with_matches<M: Matcher>(
    searcher: &mut Searcher,
    matcher: &M,
    files: &[PathBuf],
    limit: usize,
) -> Result<String, String> {
    let mut matching_files = Vec::new();

    for file in files {
        if matching_files.len() >= limit {
            break;
        }

        let mut found = false;
        let sink = grep_searcher::sinks::UTF8(|_line_num, _line| {
            found = true;
            Ok(false) // Stop after first match
        });

        // Ignore errors for individual files (binary files, permission issues, etc.)
        let _ = searcher.search_path(matcher, file, sink);

        if found {
            matching_files.push(file.display().to_string());
        }
    }

    if matching_files.is_empty() {
        Ok("No matches found".to_string())
    } else {
        Ok(matching_files.join("\n"))
    }
}

/// Search for matching content (content mode).
fn search_content<M: Matcher>(
    searcher: &mut Searcher,
    matcher: &M,
    files: &[PathBuf],
    show_line_numbers: bool,
    limit: usize,
) -> Result<String, String> {
    let mut output = String::new();
    let mut total_matches = 0;

    for file in files {
        if total_matches >= limit {
            break;
        }

        let mut file_output = String::new();
        let mut file_matches = 0;
        let file_path = file.clone();

        let sink = grep_searcher::sinks::UTF8(|line_num, line| {
            if total_matches + file_matches >= limit {
                return Ok(false);
            }

            if show_line_numbers {
                file_output.push_str(&format!(
                    "{}:{}: {}",
                    file_path.display(),
                    line_num,
                    line.trim_end()
                ));
            } else {
                file_output.push_str(&format!("{}: {}", file_path.display(), line.trim_end()));
            }
            file_output.push('\n');
            file_matches += 1;

            Ok(true)
        });

        // Ignore errors for individual files
        let _ = searcher.search_path(matcher, file, sink);

        if file_matches > 0 {
            output.push_str(&file_output);
            total_matches += file_matches;
        }
    }

    if output.is_empty() {
        Ok("No matches found".to_string())
    } else {
        Ok(output.trim_end().to_string())
    }
}

/// Search and count matches per file (count mode).
fn search_count<M: Matcher>(
    searcher: &mut Searcher,
    matcher: &M,
    files: &[PathBuf],
    limit: usize,
) -> Result<String, String> {
    let mut results = Vec::new();

    for file in files {
        if results.len() >= limit {
            break;
        }

        let mut count = 0u64;
        let sink = grep_searcher::sinks::UTF8(|_line_num, _line| {
            count += 1;
            Ok(true)
        });

        // Ignore errors for individual files
        let _ = searcher.search_path(matcher, file, sink);

        if count > 0 {
            results.push(format!("{}:{}", file.display(), count));
        }
    }

    if results.is_empty() {
        Ok("No matches found".to_string())
    } else {
        Ok(results.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::PermissionPanelResponse;
    use crate::controller::types::ControllerEvent;
    use crate::permissions::GrantTarget;
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

    fn setup_test_files() -> TempDir {
        let temp = TempDir::new().unwrap();

        fs::write(
            temp.path().join("test.rs"),
            r#"fn main() {
    let error = "something wrong";
    println!("Error: {}", error);
}
"#,
        )
        .unwrap();

        fs::write(
            temp.path().join("lib.rs"),
            r#"pub fn handle_error(e: Error) {
    eprintln!("Error occurred: {}", e);
}
"#,
        )
        .unwrap();

        fs::write(
            temp.path().join("test.js"),
            r#"function handleError(err) {
    console.error("Error:", err);
}
"#,
        )
        .unwrap();

        temp
    }

    #[tokio::test]
    async fn test_simple_search_with_permission() {
        let temp = setup_test_files();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GrepTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("error".to_string()),
        );
        input.insert("-i".to_string(), serde_json::Value::Bool(true));

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-grep-1".to_string(),
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
        let output = result.unwrap();
        // Should find matches in multiple files
        assert!(output.contains("test.rs") || output.contains("lib.rs"));
    }

    #[tokio::test]
    async fn test_search_permission_denied() {
        let temp = setup_test_files();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GrepTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("error".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-grep-denied".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        // Deny permission
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
    async fn test_content_mode() {
        let temp = setup_test_files();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GrepTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("Error".to_string()),
        );
        input.insert(
            "output_mode".to_string(),
            serde_json::Value::String("content".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-grep-content".to_string(),
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
        // Content mode should include line content
        assert!(output.contains("Error"));
    }

    #[tokio::test]
    async fn test_count_mode() {
        let temp = setup_test_files();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GrepTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("Error".to_string()),
        );
        input.insert(
            "output_mode".to_string(),
            serde_json::Value::String("count".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-grep-count".to_string(),
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
        // Count mode should include file paths with counts
        assert!(output.contains(":"));
    }

    #[tokio::test]
    async fn test_type_filter() {
        let temp = setup_test_files();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GrepTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("function".to_string()),
        );
        input.insert(
            "type".to_string(),
            serde_json::Value::String("js".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-grep-type".to_string(),
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
        // Should only find matches in .js files
        assert!(output.contains("test.js"));
        assert!(!output.contains(".rs"));
    }

    #[tokio::test]
    async fn test_invalid_pattern() {
        let temp = setup_test_files();
        let (registry, mut event_rx) = create_test_registry();
        let tool = GrepTool::with_default_path(registry.clone(), temp.path().to_path_buf());

        let mut input = HashMap::new();
        // Invalid regex pattern (unbalanced parentheses)
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("(invalid".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-grep-invalid".to_string(),
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
        assert!(result.unwrap_err().contains("Invalid regex pattern"));
    }

    #[tokio::test]
    async fn test_missing_pattern() {
        let (registry, _event_rx) = create_test_registry();
        let tool = GrepTool::new(registry);

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
        let tool = GrepTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("test".to_string()),
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
        let tool = GrepTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String("impl.*Trait".to_string()),
        );

        let result = "file1.rs\nfile2.rs\nfile3.rs";
        let summary = tool.compact_summary(&input, result);
        assert_eq!(summary, "[Grep: 'impl.*Trait' (3 matches)]");
    }

    #[test]
    fn test_compact_summary_long_pattern() {
        let (registry, _event_rx) = create_test_registry();
        let tool = GrepTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "pattern".to_string(),
            serde_json::Value::String(
                "this_is_a_very_long_pattern_that_should_be_truncated".to_string(),
            ),
        );

        let result = "file1.rs";
        let summary = tool.compact_summary(&input, result);
        assert!(summary.contains("..."));
        assert!(summary.len() < 100);
    }

    #[test]
    fn test_build_permission_request() {
        let request = GrepTool::build_permission_request("test-tool-id", "/path/to/src");

        assert_eq!(request.description, "Search files in: /path/to/src");
        assert_eq!(
            request.reason,
            Some("Search file contents using grep".to_string())
        );
        assert_eq!(request.target, GrantTarget::path("/path/to/src", true));
        assert_eq!(request.required_level, PermissionLevel::Read);
    }

    #[test]
    fn test_get_type_extensions() {
        assert_eq!(GrepTool::get_type_extensions("rust"), vec!["rs"]);
        assert_eq!(
            GrepTool::get_type_extensions("js"),
            vec!["js", "mjs", "cjs"]
        );
        assert_eq!(GrepTool::get_type_extensions("py"), vec!["py", "pyi"]);
        assert!(GrepTool::get_type_extensions("unknown").is_empty());
    }
}
