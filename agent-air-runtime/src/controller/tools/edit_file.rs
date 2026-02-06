//! EditFile tool implementation for find-and-replace operations.
//!
//! This tool performs string replacement in files with optional fuzzy matching.
//! It integrates with the PermissionRegistry to require user approval before
//! modifying files.

use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use strsim::normalized_levenshtein;

use super::types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};
use crate::permissions::{GrantTarget, PermissionLevel, PermissionRegistry, PermissionRequest};

/// EditFile tool name constant.
pub const EDIT_FILE_TOOL_NAME: &str = "edit_file";

/// EditFile tool description constant.
pub const EDIT_FILE_TOOL_DESCRIPTION: &str = r#"Performs string replacement in a file with optional fuzzy matching.

Usage:
- The file_path parameter must be an absolute path, not a relative path
- The old_string must be found in the file (or fuzzy matched if enabled)
- The new_string will replace the old_string
- By default, only the first occurrence is replaced
- Use replace_all to replace all occurrences
- Fuzzy matching helps handle whitespace differences and minor variations

Returns:
- Success message with number of replacements made
- Error if old_string is not found in the file
- Error if file doesn't exist or cannot be read"#;

/// EditFile tool JSON schema constant.
pub const EDIT_FILE_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "file_path": {
            "type": "string",
            "description": "The absolute path to the file to edit"
        },
        "old_string": {
            "type": "string",
            "description": "The string to find and replace"
        },
        "new_string": {
            "type": "string",
            "description": "The string to replace with"
        },
        "replace_all": {
            "type": "boolean",
            "description": "Whether to replace all occurrences or just the first. Defaults to false."
        },
        "fuzzy_match": {
            "type": "boolean",
            "description": "Enable fuzzy matching for whitespace-insensitive matching. Defaults to false."
        },
        "fuzzy_threshold": {
            "type": "number",
            "description": "Similarity threshold for fuzzy matching (0.0 to 1.0). Defaults to 0.7."
        }
    },
    "required": ["file_path", "old_string", "new_string"]
}"#;

/// Configuration for fuzzy matching behavior.
#[derive(Debug, Clone)]
pub struct FuzzyConfig {
    /// Minimum similarity threshold (0.0 to 1.0).
    pub threshold: f64,
    /// Whether to normalize whitespace before comparison.
    pub normalize_whitespace: bool,
}

impl Default for FuzzyConfig {
    fn default() -> Self {
        Self {
            threshold: 0.7,
            normalize_whitespace: true,
        }
    }
}

/// Type of match found during search.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatchType {
    /// Exact string match.
    Exact,
    /// Match after normalizing whitespace.
    WhitespaceInsensitive,
    /// Fuzzy match using similarity scoring.
    Fuzzy,
}

/// Tool that performs find-and-replace operations on files.
pub struct EditFileTool {
    /// Reference to the permission registry for requesting write permissions.
    permission_registry: Arc<PermissionRegistry>,
}

impl EditFileTool {
    /// Create a new EditFileTool with permission registry.
    pub fn new(permission_registry: Arc<PermissionRegistry>) -> Self {
        Self {
            permission_registry,
        }
    }

    /// Build a permission request for editing a file.
    fn build_permission_request(
        tool_use_id: &str,
        file_path: &str,
        old_string: &str,
    ) -> PermissionRequest {
        let path = file_path;
        let truncated_old = truncate_string(old_string, 30);
        let reason = format!("Replace '{}' in file", truncated_old);

        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(path, false),
            PermissionLevel::Write,
            format!("Edit file: {}", path),
        )
        .with_reason(reason)
        .with_tool(EDIT_FILE_TOOL_NAME)
    }

    /// Normalize whitespace for fuzzy comparison.
    fn normalize_whitespace(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Multi-stage fuzzy matching.
    /// Returns: Option<(start_byte, end_byte, similarity_score, match_type)>
    fn find_match(
        content: &str,
        search: &str,
        config: &FuzzyConfig,
    ) -> Option<(usize, usize, f64, MatchType)> {
        // Stage 1: Exact match
        if let Some(start) = content.find(search) {
            return Some((start, start + search.len(), 1.0, MatchType::Exact));
        }

        // Stage 2: Whitespace-insensitive exact match
        if let Some(pos) = Self::find_normalized_position(content, search) {
            return Some((pos.0, pos.1, 0.95, MatchType::WhitespaceInsensitive));
        }

        // Stage 3: Fuzzy match using sliding window
        Self::find_fuzzy_match_sliding_window(content, search, config)
            .map(|(start, end, sim)| (start, end, sim, MatchType::Fuzzy))
    }

    /// Find position in original content that corresponds to normalized match.
    fn find_normalized_position(content: &str, search: &str) -> Option<(usize, usize)> {
        let search_lines: Vec<&str> = search.lines().collect();
        let content_lines: Vec<&str> = content.lines().collect();

        if search_lines.is_empty() {
            return None;
        }

        // Find matching line sequence
        let first_search_normalized = Self::normalize_whitespace(search_lines[0]);

        for (i, content_line) in content_lines.iter().enumerate() {
            let content_normalized = Self::normalize_whitespace(content_line);

            if content_normalized == first_search_normalized {
                // Check if subsequent lines match
                let mut all_match = true;
                for (j, search_line) in search_lines.iter().enumerate().skip(1) {
                    if i + j >= content_lines.len() {
                        all_match = false;
                        break;
                    }
                    let cn = Self::normalize_whitespace(content_lines[i + j]);
                    let sn = Self::normalize_whitespace(search_line);
                    if cn != sn {
                        all_match = false;
                        break;
                    }
                }

                if all_match {
                    // Calculate byte positions
                    let start_byte: usize = content_lines[..i]
                        .iter()
                        .map(|l| l.len() + 1) // +1 for newline
                        .sum();
                    let end_line = i + search_lines.len();
                    let matched_text = content_lines[i..end_line].join("\n");
                    let end_byte = start_byte + matched_text.len();

                    return Some((start_byte, end_byte));
                }
            }
        }

        None
    }

    /// Sliding window fuzzy match with similarity scoring.
    fn find_fuzzy_match_sliding_window(
        content: &str,
        search: &str,
        config: &FuzzyConfig,
    ) -> Option<(usize, usize, f64)> {
        let search_lines: Vec<&str> = search.lines().collect();
        let content_lines: Vec<&str> = content.lines().collect();
        let search_line_count = search_lines.len();

        if search_line_count == 0 || content_lines.len() < search_line_count {
            return None;
        }

        let mut best_match: Option<(usize, usize, f64)> = None;

        // Slide window of search_line_count lines over content
        for window_start in 0..=(content_lines.len() - search_line_count) {
            let window_end = window_start + search_line_count;
            let window: Vec<&str> = content_lines[window_start..window_end].to_vec();

            // Calculate similarity
            let window_text = if config.normalize_whitespace {
                Self::normalize_whitespace(&window.join("\n"))
            } else {
                window.join("\n")
            };

            let search_text = if config.normalize_whitespace {
                Self::normalize_whitespace(search)
            } else {
                search.to_string()
            };

            let similarity = normalized_levenshtein(&search_text, &window_text);

            if similarity >= config.threshold
                && (best_match.is_none() || similarity > best_match.unwrap().2)
            {
                // Calculate byte positions
                let start_byte: usize = content_lines[..window_start]
                    .iter()
                    .map(|l| l.len() + 1)
                    .sum();

                let matched_text = content_lines[window_start..window_end].join("\n");
                let end_byte = start_byte + matched_text.len();

                best_match = Some((start_byte, end_byte, similarity));
            }
        }

        best_match
    }

    /// Find all exact matches.
    fn find_all_exact_matches(content: &str, search: &str) -> Vec<(usize, usize)> {
        let mut matches = Vec::new();
        let mut start = 0;

        while let Some(pos) = content[start..].find(search) {
            let actual_start = start + pos;
            matches.push((actual_start, actual_start + search.len()));
            start = actual_start + search.len();
        }

        matches
    }
}

impl Executable for EditFileTool {
    fn name(&self) -> &str {
        EDIT_FILE_TOOL_NAME
    }

    fn description(&self) -> &str {
        EDIT_FILE_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        EDIT_FILE_TOOL_SCHEMA
    }

    fn tool_type(&self) -> ToolType {
        ToolType::TextEdit
    }

    fn execute(
        &self,
        context: ToolContext,
        input: HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let permission_registry = self.permission_registry.clone();

        Box::pin(async move {
            // Extract required parameters
            let file_path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'file_path' parameter".to_string())?;

            let old_string = input
                .get("old_string")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'old_string' parameter".to_string())?;

            let new_string = input
                .get("new_string")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'new_string' parameter".to_string())?;

            // Optional parameters
            let replace_all = input
                .get("replace_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let fuzzy_match = input
                .get("fuzzy_match")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let fuzzy_threshold = input
                .get("fuzzy_threshold")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.7)
                .clamp(0.0, 1.0);

            let path = PathBuf::from(file_path);

            // Validate absolute path
            if !path.is_absolute() {
                return Err(format!(
                    "file_path must be an absolute path, got: {}",
                    file_path
                ));
            }

            // Check file exists
            if !path.exists() {
                return Err(format!("File does not exist: {}", file_path));
            }

            // Check if old_string and new_string are the same
            if old_string == new_string {
                return Err("old_string and new_string are identical".to_string());
            }

            // Request permission if not pre-approved by batch executor
            if !context.permissions_pre_approved {
                let permission_request =
                    Self::build_permission_request(&context.tool_use_id, file_path, old_string);
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
                        "Permission denied to edit '{}': {}",
                        file_path, reason
                    ));
                }
            }

            // Read file content
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read file '{}': {}", file_path, e))?;

            // Perform replacement
            let (new_content, replacement_count, match_info) = if fuzzy_match {
                let config = FuzzyConfig {
                    threshold: fuzzy_threshold,
                    normalize_whitespace: true,
                };

                if let Some((start, end, similarity, match_type)) =
                    Self::find_match(&content, old_string, &config)
                {
                    let mut new_content = String::with_capacity(content.len());
                    new_content.push_str(&content[..start]);
                    new_content.push_str(new_string);
                    new_content.push_str(&content[end..]);

                    let match_info = format!(
                        " (match type: {:?}, similarity: {:.1}%)",
                        match_type,
                        similarity * 100.0
                    );
                    (new_content, 1, match_info)
                } else {
                    return Err(format!(
                        "No match found for '{}' with threshold {:.0}%",
                        truncate_string(old_string, 50),
                        fuzzy_threshold * 100.0
                    ));
                }
            } else if replace_all {
                // Exact match - replace all
                let matches = Self::find_all_exact_matches(&content, old_string);
                if matches.is_empty() {
                    return Err(format!(
                        "String not found in file: '{}'",
                        truncate_string(old_string, 50)
                    ));
                }
                let new_content = content.replace(old_string, new_string);
                (new_content, matches.len(), String::new())
            } else {
                // Exact match - replace first only
                if let Some(start) = content.find(old_string) {
                    let end = start + old_string.len();
                    let mut new_content = String::with_capacity(content.len());
                    new_content.push_str(&content[..start]);
                    new_content.push_str(new_string);
                    new_content.push_str(&content[end..]);
                    (new_content, 1, String::new())
                } else {
                    return Err(format!(
                        "String not found in file: '{}'",
                        truncate_string(old_string, 50)
                    ));
                }
            };

            // Write modified content back
            fs::write(&path, &new_content)
                .map_err(|e| format!("Failed to write file '{}': {}", file_path, e))?;

            Ok(format!(
                "Successfully made {} replacement(s) in '{}'{}",
                replacement_count, file_path, match_info
            ))
        })
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Edit File".to_string(),
            display_title: Box::new(|input| {
                input
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .map(|p| {
                        Path::new(p)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(p)
                            .to_string()
                    })
                    .unwrap_or_default()
            }),
            display_content: Box::new(|input, result| {
                let old_str = input
                    .get("old_string")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let new_str = input
                    .get("new_string")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let content = format!(
                    "--- old\n+++ new\n- {}\n+ {}\n\n{}",
                    truncate_string(old_str, 100),
                    truncate_string(new_str, 100),
                    result
                );

                DisplayResult {
                    content,
                    content_type: ResultContentType::PlainText,
                    is_truncated: false,
                    full_length: 0,
                }
            }),
        }
    }

    fn compact_summary(&self, input: &HashMap<String, serde_json::Value>, result: &str) -> String {
        let filename = input
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(p)
            })
            .unwrap_or("unknown");

        let status = if result.contains("Successfully") {
            "ok"
        } else {
            "error"
        };

        format!("[EditFile: {} ({})]", filename, status)
    }

    fn required_permissions(
        &self,
        context: &ToolContext,
        input: &HashMap<String, serde_json::Value>,
    ) -> Option<Vec<PermissionRequest>> {
        // Extract file_path from input
        let file_path = input.get("file_path").and_then(|v| v.as_str())?;

        // Extract old_string for permission request context
        let old_string = input
            .get("old_string")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Validate that the path is absolute
        let path = PathBuf::from(file_path);
        if !path.is_absolute() {
            return None;
        }

        // Build the permission request using the existing helper method
        let permission_request =
            Self::build_permission_request(&context.tool_use_id, file_path, old_string);

        Some(vec![permission_request])
    }
}

/// Truncate string for display purposes.
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::types::ControllerEvent;
    use crate::permissions::PermissionLevel;
    use crate::permissions::PermissionPanelResponse;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

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

    #[tokio::test]
    async fn test_exact_replace_first() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = EditFileTool::new(registry.clone());

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "foo bar foo baz").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "old_string".to_string(),
            serde_json::Value::String("foo".to_string()),
        );
        input.insert(
            "new_string".to_string(),
            serde_json::Value::String("qux".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-edit-1".to_string(),
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
        assert!(result.unwrap().contains("1 replacement"));
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "qux bar foo baz");
    }

    #[tokio::test]
    async fn test_exact_replace_all() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = EditFileTool::new(registry.clone());

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "foo bar foo baz foo").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "old_string".to_string(),
            serde_json::Value::String("foo".to_string()),
        );
        input.insert(
            "new_string".to_string(),
            serde_json::Value::String("qux".to_string()),
        );
        input.insert("replace_all".to_string(), serde_json::Value::Bool(true));

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-edit-2".to_string(),
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
        assert!(result.unwrap().contains("3 replacement"));
        assert_eq!(
            fs::read_to_string(&file_path).unwrap(),
            "qux bar qux baz qux"
        );
    }

    #[tokio::test]
    async fn test_string_not_found() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = EditFileTool::new(registry.clone());

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "old_string".to_string(),
            serde_json::Value::String("notfound".to_string()),
        );
        input.insert(
            "new_string".to_string(),
            serde_json::Value::String("replacement".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-edit-3".to_string(),
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
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let (registry, _event_rx) = create_test_registry();
        let tool = EditFileTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String("/nonexistent/file.txt".to_string()),
        );
        input.insert(
            "old_string".to_string(),
            serde_json::Value::String("foo".to_string()),
        );
        input.insert(
            "new_string".to_string(),
            serde_json::Value::String("bar".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-edit-4".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_relative_path_rejected() {
        let (registry, _event_rx) = create_test_registry();
        let tool = EditFileTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String("relative/path.txt".to_string()),
        );
        input.insert(
            "old_string".to_string(),
            serde_json::Value::String("foo".to_string()),
        );
        input.insert(
            "new_string".to_string(),
            serde_json::Value::String("bar".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-edit-5".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("absolute path"));
    }

    #[tokio::test]
    async fn test_identical_strings_rejected() {
        let (registry, _event_rx) = create_test_registry();
        let tool = EditFileTool::new(registry);

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "old_string".to_string(),
            serde_json::Value::String("same".to_string()),
        );
        input.insert(
            "new_string".to_string(),
            serde_json::Value::String("same".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-edit-6".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("identical"));
    }

    #[tokio::test]
    async fn test_permission_denied() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = EditFileTool::new(registry.clone());

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "old_string".to_string(),
            serde_json::Value::String("hello".to_string()),
        );
        input.insert(
            "new_string".to_string(),
            serde_json::Value::String("goodbye".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-edit-7".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let registry_clone = registry.clone();
        tokio::spawn(async move {
            if let Some(ControllerEvent::PermissionRequired { tool_use_id, .. }) =
                event_rx.recv().await
            {
                registry_clone
                    .respond_to_request(&tool_use_id, deny("Not allowed"))
                    .await
                    .unwrap();
            }
        });

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Permission denied"));
    }

    #[tokio::test]
    async fn test_whitespace_insensitive_match() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = EditFileTool::new(registry.clone());

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        // File has extra spaces
        fs::write(&file_path, "fn  foo()  {\n    bar();\n}").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        // Search string has normalized whitespace
        input.insert(
            "old_string".to_string(),
            serde_json::Value::String("fn foo() {\nbar();\n}".to_string()),
        );
        input.insert(
            "new_string".to_string(),
            serde_json::Value::String("fn foo() {\n    baz();\n}".to_string()),
        );
        input.insert("fuzzy_match".to_string(), serde_json::Value::Bool(true));

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-edit-8".to_string(),
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
        let result_str = result.unwrap();
        assert!(result_str.contains("1 replacement"));
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(
            EditFileTool::normalize_whitespace("  hello   world  "),
            "hello world"
        );
        assert_eq!(EditFileTool::normalize_whitespace("a\n\nb\tc"), "a b c");
    }

    #[test]
    fn test_find_all_exact_matches() {
        let content = "foo bar foo baz foo";
        let matches = EditFileTool::find_all_exact_matches(content, "foo");
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0], (0, 3));
        assert_eq!(matches[1], (8, 11));
        assert_eq!(matches[2], (16, 19));
    }

    #[test]
    fn test_compact_summary() {
        let (registry, _rx) = create_test_registry();
        let tool = EditFileTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String("/path/to/file.rs".to_string()),
        );

        let result = "Successfully made 2 replacement(s) in '/path/to/file.rs'";
        let summary = tool.compact_summary(&input, result);
        assert_eq!(summary, "[EditFile: file.rs (ok)]");
    }

    #[test]
    fn test_build_permission_request() {
        let request = EditFileTool::build_permission_request(
            "test-tool-use-id",
            "/path/to/file.rs",
            "old code",
        );
        assert_eq!(request.description, "Edit file: /path/to/file.rs");
        assert!(request.reason.unwrap().contains("old code"));
        assert_eq!(request.target, GrantTarget::path("/path/to/file.rs", false));
        assert_eq!(request.required_level, PermissionLevel::Write);
    }
}
