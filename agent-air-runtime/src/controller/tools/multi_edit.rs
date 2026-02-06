//! MultiEdit tool implementation for atomic multiple find-and-replace operations.
//!
//! This tool performs multiple string replacements on a single file atomically.
//! All edits are validated before any are applied, ensuring the file is either
//! fully updated or unchanged if any edit fails.

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

/// MultiEdit tool name constant.
pub const MULTI_EDIT_TOOL_NAME: &str = "multi_edit";

/// MultiEdit tool description constant.
pub const MULTI_EDIT_TOOL_DESCRIPTION: &str = r#"Performs multiple find-and-replace operations on a single file atomically.

Usage:
- The file_path parameter must be an absolute path
- Provide an array of edits, each with old_string and new_string
- All edits are validated before any are applied
- If any edit fails validation, no changes are made
- Edits are applied in order, accounting for position shifts

Features:
- Single permission request for all edits
- Atomic: all edits succeed or none are applied
- Automatic position adjustment as earlier edits shift content
- Optional fuzzy matching per edit
- Dry-run mode to preview changes

Returns:
- Success message with count of edits applied
- Error if any edit cannot be applied (no changes made)"#;

/// MultiEdit tool JSON schema constant.
pub const MULTI_EDIT_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "file_path": {
            "type": "string",
            "description": "The absolute path to the file to edit"
        },
        "edits": {
            "type": "array",
            "description": "Array of edit operations to apply in order",
            "items": {
                "type": "object",
                "properties": {
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
                        "description": "Replace all occurrences (default: false, first only)"
                    },
                    "fuzzy_match": {
                        "type": "boolean",
                        "description": "Enable fuzzy matching for this edit (default: false)"
                    },
                    "fuzzy_threshold": {
                        "type": "number",
                        "description": "Similarity threshold for fuzzy matching (0.0-1.0, default: 0.7)"
                    }
                },
                "required": ["old_string", "new_string"]
            },
            "minItems": 1,
            "maxItems": 50
        },
        "dry_run": {
            "type": "boolean",
            "description": "If true, validate edits and return preview without applying. Default: false"
        }
    },
    "required": ["file_path", "edits"]
}"#;

const MAX_EDITS: usize = 50;

/// Configuration for fuzzy matching behavior.
#[derive(Debug, Clone)]
struct FuzzyConfig {
    threshold: f64,
    normalize_whitespace: bool,
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
enum MatchType {
    Exact,
    WhitespaceInsensitive,
    Fuzzy,
}

/// Input for a single edit operation.
#[derive(Debug, Clone)]
struct EditInput {
    old_string: String,
    new_string: String,
    replace_all: bool,
    fuzzy_match: bool,
    fuzzy_threshold: f64,
}

/// A planned edit with resolved byte positions.
#[derive(Debug, Clone)]
struct PlannedEdit {
    /// Index in the original edits array (for error reporting).
    edit_index: usize,
    /// Start byte position in original content.
    start: usize,
    /// End byte position in original content.
    end: usize,
    /// The replacement string.
    new_string: String,
    /// Match type for reporting.
    match_type: MatchType,
    /// Similarity score (1.0 for exact).
    similarity: f64,
}

/// Tool that performs multiple find-and-replace operations atomically.
pub struct MultiEditTool {
    permission_registry: Arc<PermissionRegistry>,
}

impl MultiEditTool {
    /// Create a new MultiEditTool with permission registry.
    pub fn new(permission_registry: Arc<PermissionRegistry>) -> Self {
        Self {
            permission_registry,
        }
    }

    /// Build a permission request for multi-edit operation.
    fn build_permission_request(
        tool_use_id: &str,
        file_path: &str,
        edit_count: usize,
    ) -> PermissionRequest {
        let path = file_path;
        let reason = format!("Apply {} find-and-replace operations", edit_count);

        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(path, false),
            PermissionLevel::Write,
            format!("Multi-edit file: {}", path),
        )
        .with_reason(reason)
        .with_tool(MULTI_EDIT_TOOL_NAME)
    }

    /// Normalize whitespace for fuzzy comparison.
    fn normalize_whitespace(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Find the first exact match.
    fn find_first_exact_match(content: &str, search: &str) -> Vec<(usize, usize, f64, MatchType)> {
        if let Some(start) = content.find(search) {
            vec![(start, start + search.len(), 1.0, MatchType::Exact)]
        } else {
            vec![]
        }
    }

    /// Find all exact matches.
    fn find_all_exact_matches(content: &str, search: &str) -> Vec<(usize, usize, f64, MatchType)> {
        let mut matches = Vec::new();
        let mut start = 0;

        while let Some(pos) = content[start..].find(search) {
            let actual_start = start + pos;
            matches.push((
                actual_start,
                actual_start + search.len(),
                1.0,
                MatchType::Exact,
            ));
            start = actual_start + search.len();
        }

        matches
    }

    /// Find match using fuzzy matching.
    fn find_fuzzy_match(
        content: &str,
        search: &str,
        config: &FuzzyConfig,
    ) -> Vec<(usize, usize, f64, MatchType)> {
        // Stage 1: Try exact match first
        if let Some(start) = content.find(search) {
            return vec![(start, start + search.len(), 1.0, MatchType::Exact)];
        }

        // Stage 2: Try whitespace-insensitive match
        if let Some((start, end)) = Self::find_normalized_position(content, search) {
            return vec![(start, end, 0.95, MatchType::WhitespaceInsensitive)];
        }

        // Stage 3: Fuzzy match using sliding window
        if let Some((start, end, similarity)) =
            Self::find_fuzzy_match_sliding_window(content, search, config)
        {
            return vec![(start, end, similarity, MatchType::Fuzzy)];
        }

        vec![]
    }

    /// Find position in original content that corresponds to normalized match.
    fn find_normalized_position(content: &str, search: &str) -> Option<(usize, usize)> {
        let search_lines: Vec<&str> = search.lines().collect();
        let content_lines: Vec<&str> = content.lines().collect();

        if search_lines.is_empty() {
            return None;
        }

        let first_search_normalized = Self::normalize_whitespace(search_lines[0]);

        for (i, content_line) in content_lines.iter().enumerate() {
            let content_normalized = Self::normalize_whitespace(content_line);

            if content_normalized == first_search_normalized {
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
                    let start_byte: usize = content_lines[..i].iter().map(|l| l.len() + 1).sum();
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

        for window_start in 0..=(content_lines.len() - search_line_count) {
            let window_end = window_start + search_line_count;
            let window: Vec<&str> = content_lines[window_start..window_end].to_vec();

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

    /// Plan all edits without applying them.
    fn plan_edits(content: &str, edits: &[EditInput]) -> Result<Vec<PlannedEdit>, String> {
        let mut planned = Vec::new();

        for (index, edit) in edits.iter().enumerate() {
            let config = FuzzyConfig {
                threshold: edit.fuzzy_threshold,
                normalize_whitespace: true,
            };

            let matches = if edit.fuzzy_match {
                Self::find_fuzzy_match(content, &edit.old_string, &config)
            } else if edit.replace_all {
                Self::find_all_exact_matches(content, &edit.old_string)
            } else {
                Self::find_first_exact_match(content, &edit.old_string)
            };

            if matches.is_empty() {
                return Err(format!(
                    "Edit {}: string not found: '{}'",
                    index + 1,
                    truncate_string(&edit.old_string, 50)
                ));
            }

            for (start, end, similarity, match_type) in matches {
                planned.push(PlannedEdit {
                    edit_index: index,
                    start,
                    end,
                    new_string: edit.new_string.clone(),
                    match_type,
                    similarity,
                });
            }
        }

        // Validate no overlaps
        Self::validate_no_overlaps(&planned)?;

        Ok(planned)
    }

    /// Check if two edits overlap.
    fn edits_overlap(a: &PlannedEdit, b: &PlannedEdit) -> bool {
        a.start < b.end && b.start < a.end
    }

    /// Validate that no edits have overlapping regions.
    fn validate_no_overlaps(edits: &[PlannedEdit]) -> Result<(), String> {
        for i in 0..edits.len() {
            for j in (i + 1)..edits.len() {
                if Self::edits_overlap(&edits[i], &edits[j]) {
                    return Err(format!(
                        "Edits {} and {} have overlapping regions",
                        edits[i].edit_index + 1,
                        edits[j].edit_index + 1
                    ));
                }
            }
        }
        Ok(())
    }

    /// Apply planned edits and return new content.
    fn apply_edits(content: &str, mut edits: Vec<PlannedEdit>) -> String {
        // Sort by position descending (apply end-to-start)
        edits.sort_by(|a, b| b.start.cmp(&a.start));

        let mut result = content.to_string();
        for edit in edits {
            result.replace_range(edit.start..edit.end, &edit.new_string);
        }
        result
    }

    /// Parse edits from JSON value.
    fn parse_edits(value: &serde_json::Value) -> Result<Vec<EditInput>, String> {
        let array = value.as_array().ok_or("'edits' must be an array")?;

        array
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let obj = v
                    .as_object()
                    .ok_or_else(|| format!("Edit {} must be an object", i + 1))?;

                Ok(EditInput {
                    old_string: obj
                        .get("old_string")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| format!("Edit {}: missing 'old_string'", i + 1))?
                        .to_string(),
                    new_string: obj
                        .get("new_string")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| format!("Edit {}: missing 'new_string'", i + 1))?
                        .to_string(),
                    replace_all: obj
                        .get("replace_all")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    fuzzy_match: obj
                        .get("fuzzy_match")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    fuzzy_threshold: obj
                        .get("fuzzy_threshold")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.7)
                        .clamp(0.0, 1.0),
                })
            })
            .collect()
    }
}

impl Executable for MultiEditTool {
    fn name(&self) -> &str {
        MULTI_EDIT_TOOL_NAME
    }

    fn description(&self) -> &str {
        MULTI_EDIT_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        MULTI_EDIT_TOOL_SCHEMA
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
            // Extract parameters
            let file_path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'file_path' parameter".to_string())?;

            let edits_value = input
                .get("edits")
                .ok_or_else(|| "Missing required 'edits' parameter".to_string())?;

            let edits = Self::parse_edits(edits_value)?;

            let dry_run = input
                .get("dry_run")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            // Validate inputs
            if edits.is_empty() {
                return Err("No edits provided".to_string());
            }
            if edits.len() > MAX_EDITS {
                return Err(format!(
                    "Too many edits: {} (max {})",
                    edits.len(),
                    MAX_EDITS
                ));
            }

            let path = PathBuf::from(file_path);
            if !path.is_absolute() {
                return Err(format!(
                    "file_path must be an absolute path, got: {}",
                    file_path
                ));
            }
            if !path.exists() {
                return Err(format!("File does not exist: {}", file_path));
            }

            // Validate no identical old/new strings
            for (i, edit) in edits.iter().enumerate() {
                if edit.old_string == edit.new_string {
                    return Err(format!(
                        "Edit {}: old_string and new_string are identical",
                        i + 1
                    ));
                }
            }

            // Read file
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read file '{}': {}", file_path, e))?;

            // Plan edits (validates all can be applied)
            let planned = Self::plan_edits(&content, &edits)?;

            // Generate new content
            let new_content = Self::apply_edits(&content, planned.clone());

            // Dry run - return preview without applying
            if dry_run {
                let fuzzy_count = planned
                    .iter()
                    .filter(|e| e.match_type != MatchType::Exact)
                    .count();

                let mut summary = format!(
                    "Dry run: {} edit(s) would be applied to '{}'\n",
                    planned.len(),
                    file_path
                );

                if fuzzy_count > 0 {
                    summary.push_str(&format!("  ({} fuzzy matches)\n", fuzzy_count));
                }

                summary.push_str(&format!(
                    "\nOriginal: {} bytes\nModified: {} bytes\nDelta: {} bytes",
                    content.len(),
                    new_content.len(),
                    new_content.len() as i64 - content.len() as i64
                ));

                return Ok(summary);
            }

            // Request permission if not pre-approved by batch executor
            if !context.permissions_pre_approved {
                let permission_request =
                    Self::build_permission_request(&context.tool_use_id, file_path, edits.len());
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

            // Write new content
            fs::write(&path, &new_content)
                .map_err(|e| format!("Failed to write file '{}': {}", file_path, e))?;

            // Build success response
            let edit_count = planned.len();
            let fuzzy_edits: Vec<_> = planned
                .iter()
                .filter(|e| e.match_type != MatchType::Exact)
                .collect();

            let mut result = format!(
                "Successfully applied {} edit(s) to '{}'",
                edit_count, file_path
            );

            if !fuzzy_edits.is_empty() {
                let avg_similarity: f64 = fuzzy_edits.iter().map(|e| e.similarity).sum::<f64>()
                    / fuzzy_edits.len() as f64;
                result.push_str(&format!(
                    " ({} fuzzy matches, avg {:.0}% similarity)",
                    fuzzy_edits.len(),
                    avg_similarity * 100.0
                ));
            }

            Ok(result)
        })
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Multi-Edit".to_string(),
            display_title: Box::new(|input| {
                let file = input
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .map(|p| {
                        Path::new(p)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(p)
                    })
                    .unwrap_or("file");
                let count = input
                    .get("edits")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                format!("{} ({} edits)", file, count)
            }),
            display_content: Box::new(|_input, result| DisplayResult {
                content: result.to_string(),
                content_type: ResultContentType::PlainText,
                is_truncated: false,
                full_length: 0,
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

        let count = input
            .get("edits")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        let status = if result.contains("Successfully") {
            "ok"
        } else {
            "error"
        };

        format!("[MultiEdit: {} ({} edits, {})]", filename, count, status)
    }

    fn required_permissions(
        &self,
        _context: &ToolContext,
        input: &HashMap<String, serde_json::Value>,
    ) -> Option<Vec<PermissionRequest>> {
        // Extract file_path from input
        let file_path = input.get("file_path")?.as_str()?;

        // Extract and parse edits array
        let edits_value = input.get("edits")?;
        let edits = Self::parse_edits(edits_value).ok()?;

        // Build a single permission request for all edits
        let permission_request = Self::build_permission_request("preview", file_path, edits.len());

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
    async fn test_multiple_edits_success() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = MultiEditTool::new(registry.clone());

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "foo bar baz foo").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "edits".to_string(),
            serde_json::json!([
                {"old_string": "foo", "new_string": "qux"},
                {"old_string": "bar", "new_string": "quux"}
            ]),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-multi-1".to_string(),
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
        assert!(result.unwrap().contains("2 edit(s)"));
        // First "foo" replaced, "bar" replaced, second "foo" still there
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "qux quux baz foo");
    }

    #[tokio::test]
    async fn test_replace_all_in_multi_edit() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = MultiEditTool::new(registry.clone());

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "foo bar foo baz foo").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "edits".to_string(),
            serde_json::json!([
                {"old_string": "foo", "new_string": "qux", "replace_all": true}
            ]),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-multi-2".to_string(),
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
        assert!(result.unwrap().contains("3 edit(s)"));
        assert_eq!(
            fs::read_to_string(&file_path).unwrap(),
            "qux bar qux baz qux"
        );
    }

    #[tokio::test]
    async fn test_edit_not_found_fails_all() {
        let (registry, _event_rx) = create_test_registry();
        let tool = MultiEditTool::new(registry);

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "foo bar baz").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "edits".to_string(),
            serde_json::json!([
                {"old_string": "foo", "new_string": "qux"},
                {"old_string": "notfound", "new_string": "xxx"}
            ]),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-multi-3".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Edit 2"));
        // File should be unchanged
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "foo bar baz");
    }

    #[tokio::test]
    async fn test_overlapping_edits_rejected() {
        let (registry, _event_rx) = create_test_registry();
        let tool = MultiEditTool::new(registry);

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "foo bar baz").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "edits".to_string(),
            serde_json::json!([
                {"old_string": "foo bar", "new_string": "xxx"},
                {"old_string": "bar baz", "new_string": "yyy"}
            ]),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-multi-4".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("overlapping"));
    }

    #[tokio::test]
    async fn test_dry_run_no_changes() {
        let (registry, _event_rx) = create_test_registry();
        let tool = MultiEditTool::new(registry);

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "foo bar baz").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "edits".to_string(),
            serde_json::json!([
                {"old_string": "foo", "new_string": "qux"}
            ]),
        );
        input.insert("dry_run".to_string(), serde_json::Value::Bool(true));

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-multi-5".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Dry run"));
        // File should be unchanged
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "foo bar baz");
    }

    #[tokio::test]
    async fn test_empty_edits_rejected() {
        let (registry, _event_rx) = create_test_registry();
        let tool = MultiEditTool::new(registry);

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "foo bar").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert("edits".to_string(), serde_json::json!([]));

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-multi-6".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No edits"));
    }

    #[tokio::test]
    async fn test_identical_strings_rejected() {
        let (registry, _event_rx) = create_test_registry();
        let tool = MultiEditTool::new(registry);

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "foo bar").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "edits".to_string(),
            serde_json::json!([
                {"old_string": "foo", "new_string": "foo"}
            ]),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-multi-7".to_string(),
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
        let tool = MultiEditTool::new(registry.clone());

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "foo bar").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "edits".to_string(),
            serde_json::json!([
                {"old_string": "foo", "new_string": "qux"}
            ]),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-multi-8".to_string(),
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
        // File should be unchanged
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "foo bar");
    }

    #[tokio::test]
    async fn test_fuzzy_match_in_multi_edit() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = MultiEditTool::new(registry.clone());

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        // File has extra spaces
        fs::write(&file_path, "fn  foo()  {\n    bar();\n}").unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "edits".to_string(),
            serde_json::json!([
                {
                    "old_string": "fn foo() {\nbar();\n}",
                    "new_string": "fn foo() {\n    baz();\n}",
                    "fuzzy_match": true
                }
            ]),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-multi-9".to_string(),
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
        assert!(result_str.contains("1 edit(s)"));
        assert!(result_str.contains("fuzzy"));
    }

    #[test]
    fn test_edits_overlap() {
        let a = PlannedEdit {
            edit_index: 0,
            start: 0,
            end: 5,
            new_string: "xxx".to_string(),
            match_type: MatchType::Exact,
            similarity: 1.0,
        };
        let b = PlannedEdit {
            edit_index: 1,
            start: 3,
            end: 8,
            new_string: "yyy".to_string(),
            match_type: MatchType::Exact,
            similarity: 1.0,
        };
        let c = PlannedEdit {
            edit_index: 2,
            start: 10,
            end: 15,
            new_string: "zzz".to_string(),
            match_type: MatchType::Exact,
            similarity: 1.0,
        };

        assert!(MultiEditTool::edits_overlap(&a, &b));
        assert!(!MultiEditTool::edits_overlap(&a, &c));
        assert!(!MultiEditTool::edits_overlap(&b, &c));
    }

    #[test]
    fn test_apply_edits_reverse_order() {
        let content = "foo bar baz";
        let edits = vec![
            PlannedEdit {
                edit_index: 0,
                start: 0,
                end: 3,
                new_string: "qux".to_string(),
                match_type: MatchType::Exact,
                similarity: 1.0,
            },
            PlannedEdit {
                edit_index: 1,
                start: 8,
                end: 11,
                new_string: "quux".to_string(),
                match_type: MatchType::Exact,
                similarity: 1.0,
            },
        ];

        let result = MultiEditTool::apply_edits(content, edits);
        assert_eq!(result, "qux bar quux");
    }

    #[test]
    fn test_compact_summary() {
        let (registry, _rx) = create_test_registry();
        let tool = MultiEditTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String("/path/to/file.rs".to_string()),
        );
        input.insert(
            "edits".to_string(),
            serde_json::json!([
                {"old_string": "a", "new_string": "b"},
                {"old_string": "c", "new_string": "d"}
            ]),
        );

        let result = "Successfully applied 2 edit(s) to '/path/to/file.rs'";
        let summary = tool.compact_summary(&input, result);
        assert_eq!(summary, "[MultiEdit: file.rs (2 edits, ok)]");
    }

    #[test]
    fn test_parse_edits() {
        let value = serde_json::json!([
            {"old_string": "foo", "new_string": "bar"},
            {"old_string": "baz", "new_string": "qux", "replace_all": true, "fuzzy_match": true, "fuzzy_threshold": 0.8}
        ]);

        let edits = MultiEditTool::parse_edits(&value).unwrap();
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].old_string, "foo");
        assert_eq!(edits[0].new_string, "bar");
        assert!(!edits[0].replace_all);
        assert!(!edits[0].fuzzy_match);
        assert_eq!(edits[1].old_string, "baz");
        assert!(edits[1].replace_all);
        assert!(edits[1].fuzzy_match);
        assert!((edits[1].fuzzy_threshold - 0.8).abs() < 0.001);
    }
}
