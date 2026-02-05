//! Ls tool implementation
//!
//! This tool lists directory contents with optional detailed information,
//! filtering, and sorting. It requires permission to read directories.

use std::collections::HashMap;
use std::fs::{self, DirEntry};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use chrono::{DateTime, Local};
use globset::{Glob, GlobMatcher};

use super::types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};
use crate::permissions::{GrantTarget, PermissionLevel, PermissionRegistry, PermissionRequest};

/// Ls tool name constant.
pub const LS_TOOL_NAME: &str = "ls";

/// Ls tool description constant.
pub const LS_TOOL_DESCRIPTION: &str = r#"Lists directory contents with optional detailed information.

Usage:
- The path parameter must be an absolute path to a directory
- Returns file and directory names in the specified location
- Use long_format for detailed information (size, permissions, modified time)
- Supports filtering by pattern and showing hidden files

Options:
- long_format: Show detailed file information (size, permissions, modified time)
- include_hidden: Include files starting with '.' (default: false)
- pattern: Filter results by glob pattern (e.g., "*.rs")
- sort_by: Sort results by "name", "size", "modified" (default: "name")
- reverse: Reverse sort order (default: false)
- directories_first: List directories before files (default: true)

Examples:
- List current directory: path="/path/to/dir"
- Show hidden files: include_hidden=true
- Filter by pattern: pattern="*.rs"
- Sort by size: sort_by="size", reverse=true"#;

/// Ls tool JSON schema constant.
pub const LS_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "path": {
            "type": "string",
            "description": "The absolute path to the directory to list"
        },
        "long_format": {
            "type": "boolean",
            "description": "Show detailed information (size, permissions, modified time). Defaults to false."
        },
        "include_hidden": {
            "type": "boolean",
            "description": "Include hidden files (starting with dot). Defaults to false."
        },
        "pattern": {
            "type": "string",
            "description": "Glob pattern to filter results (e.g., '*.rs')"
        },
        "sort_by": {
            "type": "string",
            "enum": ["name", "size", "modified"],
            "description": "Field to sort by. Defaults to 'name'."
        },
        "reverse": {
            "type": "boolean",
            "description": "Reverse the sort order. Defaults to false."
        },
        "directories_first": {
            "type": "boolean",
            "description": "List directories before files. Defaults to true."
        },
        "limit": {
            "type": "integer",
            "description": "Maximum number of entries to return. Defaults to 1000."
        }
    },
    "required": ["path"]
}"#;

/// Default maximum entries to return.
const DEFAULT_LIMIT: usize = 1000;

/// Sort field for directory entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Size,
    Modified,
}

impl SortBy {
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "size" => SortBy::Size,
            "modified" | "mtime" | "time" => SortBy::Modified,
            _ => SortBy::Name,
        }
    }
}

/// A file or directory entry with metadata.
#[derive(Debug)]
struct FileEntry {
    name: String,
    is_dir: bool,
    size: u64,
    modified: SystemTime,
    #[cfg(unix)]
    permissions: u32,
    #[cfg(not(unix))]
    readonly: bool,
}

impl FileEntry {
    fn from_dir_entry(entry: &DirEntry) -> Option<Self> {
        let metadata = entry.metadata().ok()?;
        let name = entry.file_name().to_string_lossy().to_string();

        Some(Self {
            name,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            #[cfg(unix)]
            permissions: std::os::unix::fs::PermissionsExt::mode(&metadata.permissions()),
            #[cfg(not(unix))]
            readonly: metadata.permissions().readonly(),
        })
    }

    fn format_short(&self) -> String {
        if self.is_dir {
            format!("{}/", self.name)
        } else {
            self.name.clone()
        }
    }

    fn format_long(&self) -> String {
        let size_str = format_size(self.size);
        let time_str = format_time(self.modified);
        let perm_str = self.format_permissions();
        let type_char = if self.is_dir { 'd' } else { '-' };

        format!(
            "{}{} {:>8} {} {}{}",
            type_char,
            perm_str,
            size_str,
            time_str,
            self.name,
            if self.is_dir { "/" } else { "" }
        )
    }

    #[cfg(unix)]
    fn format_permissions(&self) -> String {
        let mode = self.permissions;
        let mut perm = String::with_capacity(9);

        // Owner permissions
        perm.push(if mode & 0o400 != 0 { 'r' } else { '-' });
        perm.push(if mode & 0o200 != 0 { 'w' } else { '-' });
        perm.push(if mode & 0o100 != 0 { 'x' } else { '-' });

        // Group permissions
        perm.push(if mode & 0o040 != 0 { 'r' } else { '-' });
        perm.push(if mode & 0o020 != 0 { 'w' } else { '-' });
        perm.push(if mode & 0o010 != 0 { 'x' } else { '-' });

        // Other permissions
        perm.push(if mode & 0o004 != 0 { 'r' } else { '-' });
        perm.push(if mode & 0o002 != 0 { 'w' } else { '-' });
        perm.push(if mode & 0o001 != 0 { 'x' } else { '-' });

        perm
    }

    #[cfg(not(unix))]
    fn format_permissions(&self) -> String {
        if self.readonly {
            "r--r--r--".to_string()
        } else {
            "rw-rw-rw-".to_string()
        }
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

fn format_time(time: SystemTime) -> String {
    let datetime: DateTime<Local> = time.into();
    datetime.format("%b %d %H:%M").to_string()
}

/// Tool that lists directory contents.
pub struct LsTool {
    permission_registry: Arc<PermissionRegistry>,
}

impl LsTool {
    /// Create a new LsTool instance.
    pub fn new(permission_registry: Arc<PermissionRegistry>) -> Self {
        Self {
            permission_registry,
        }
    }

    fn build_permission_request(tool_use_id: &str, path: &str) -> PermissionRequest {
        let reason = "Read directory contents";

        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(path, false),
            PermissionLevel::Read,
            format!("List directory: {}", path),
        )
        .with_reason(reason)
        .with_tool(LS_TOOL_NAME)
    }
}

impl Executable for LsTool {
    fn name(&self) -> &str {
        LS_TOOL_NAME
    }

    fn description(&self) -> &str {
        LS_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        LS_TOOL_SCHEMA
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

        Box::pin(async move {
            // Extract required parameter
            let path_str = input
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'path' parameter".to_string())?;

            let path = Path::new(path_str);

            // Validate path
            if !path.is_absolute() {
                return Err(format!("path must be an absolute path, got: {}", path_str));
            }

            if !path.exists() {
                return Err(format!("Path does not exist: {}", path_str));
            }

            if !path.is_dir() {
                return Err(format!("Path is not a directory: {}", path_str));
            }

            // Request permission if not pre-approved by batch executor
            if !context.permissions_pre_approved {
                let permission_request =
                    Self::build_permission_request(&context.tool_use_id, path_str);
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
                        .unwrap_or_else(|| "User denied".to_string());
                    return Err(format!(
                        "Permission denied to list '{}': {}",
                        path_str, reason
                    ));
                }
            }

            // Optional parameters
            let long_format = input
                .get("long_format")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let include_hidden = input
                .get("include_hidden")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let pattern = input.get("pattern").and_then(|v| v.as_str());

            let sort_by = input
                .get("sort_by")
                .and_then(|v| v.as_str())
                .map(SortBy::from_str)
                .unwrap_or(SortBy::Name);

            let reverse = input
                .get("reverse")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let directories_first = input
                .get("directories_first")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .map(|v| v.max(1) as usize)
                .unwrap_or(DEFAULT_LIMIT);

            // Build glob matcher if pattern provided
            let glob_matcher: Option<GlobMatcher> = if let Some(pat) = pattern {
                Some(
                    Glob::new(pat)
                        .map_err(|e| format!("Invalid pattern: {}", e))?
                        .compile_matcher(),
                )
            } else {
                None
            };

            // Read directory entries
            let read_dir =
                fs::read_dir(path).map_err(|e| format!("Failed to read directory: {}", e))?;

            let mut entries: Vec<FileEntry> = read_dir
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| FileEntry::from_dir_entry(&entry))
                .filter(|entry| {
                    // Filter hidden files
                    if !include_hidden && entry.name.starts_with('.') {
                        return false;
                    }

                    // Filter by pattern
                    if let Some(ref matcher) = glob_matcher
                        && !matcher.is_match(&entry.name)
                    {
                        return false;
                    }

                    true
                })
                .collect();

            // Sort entries
            entries.sort_by(|a, b| {
                // Directories first if enabled
                if directories_first && a.is_dir != b.is_dir {
                    return if a.is_dir {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    };
                }

                let cmp = match sort_by {
                    SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortBy::Size => a.size.cmp(&b.size),
                    SortBy::Modified => a.modified.cmp(&b.modified),
                };

                if reverse { cmp.reverse() } else { cmp }
            });

            // Apply limit
            let total_count = entries.len();
            entries.truncate(limit);

            // Format output
            if entries.is_empty() {
                return Ok("Directory is empty".to_string());
            }

            let mut output: Vec<String> = entries
                .iter()
                .map(|e| {
                    if long_format {
                        e.format_long()
                    } else {
                        e.format_short()
                    }
                })
                .collect();

            if total_count > limit {
                output.push(format!(
                    "\n... and {} more entries (showing {}/{})",
                    total_count - limit,
                    limit,
                    total_count
                ));
            }

            Ok(output.join("\n"))
        })
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "List Directory".to_string(),
            display_title: Box::new(|input| {
                input
                    .get("path")
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
        let dirname = input
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(p)
            })
            .unwrap_or("unknown");

        let entry_count = result
            .lines()
            .filter(|line| !line.starts_with("...") && !line.is_empty())
            .count();

        format!("[Ls: {} ({} entries)]", dirname, entry_count)
    }

    fn required_permissions(
        &self,
        context: &ToolContext,
        input: &HashMap<String, serde_json::Value>,
    ) -> Option<Vec<PermissionRequest>> {
        // Extract path parameter
        let path_str = input.get("path").and_then(|v| v.as_str())?;

        let path = Path::new(path_str);

        // Validate path is absolute, exists, and is a directory
        if !path.is_absolute() {
            return None;
        }

        if !path.exists() {
            return None;
        }

        if !path.is_dir() {
            return None;
        }

        // Build permission request using the existing helper
        let permission_request = Self::build_permission_request(&context.tool_use_id, path_str);

        Some(vec![permission_request])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    fn setup_test_dir() -> TempDir {
        let temp = TempDir::new().unwrap();

        fs::create_dir(temp.path().join("subdir")).unwrap();
        fs::write(temp.path().join("file1.txt"), "content").unwrap();
        fs::write(temp.path().join("file2.rs"), "code").unwrap();
        fs::write(temp.path().join(".hidden"), "secret").unwrap();

        temp
    }

    #[test]
    fn test_sort_by_from_str() {
        assert_eq!(SortBy::from_str("name"), SortBy::Name);
        assert_eq!(SortBy::from_str("size"), SortBy::Size);
        assert_eq!(SortBy::from_str("modified"), SortBy::Modified);
        assert_eq!(SortBy::from_str("mtime"), SortBy::Modified);
        assert_eq!(SortBy::from_str("unknown"), SortBy::Name);
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500B");
        assert_eq!(format_size(1024), "1.0K");
        assert_eq!(format_size(1536), "1.5K");
        assert_eq!(format_size(1024 * 1024), "1.0M");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0G");
    }

    #[test]
    fn test_build_permission_request() {
        let request = LsTool::build_permission_request("test-tool-id", "/home/user/project");
        assert_eq!(request.description, "List directory: /home/user/project");
        assert_eq!(request.reason, Some("Read directory contents".to_string()));
        assert_eq!(
            request.target,
            GrantTarget::path("/home/user/project", false)
        );
        assert_eq!(request.required_level, PermissionLevel::Read);
    }

    #[tokio::test]
    async fn test_ls_requires_absolute_path() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = Arc::new(PermissionRegistry::new(event_tx));
        let tool = LsTool::new(registry);

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let mut input = HashMap::new();
        input.insert(
            "path".to_string(),
            serde_json::Value::String("relative/path".to_string()),
        );

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be an absolute path"));
    }

    #[tokio::test]
    async fn test_ls_path_not_found() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = Arc::new(PermissionRegistry::new(event_tx));
        let tool = LsTool::new(registry);

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let mut input = HashMap::new();
        input.insert(
            "path".to_string(),
            serde_json::Value::String("/nonexistent/path".to_string()),
        );

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_ls_not_a_directory() {
        let temp = setup_test_dir();
        let file_path = temp.path().join("file1.txt");

        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = Arc::new(PermissionRegistry::new(event_tx));
        let tool = LsTool::new(registry);

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let mut input = HashMap::new();
        input.insert(
            "path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a directory"));
    }

    #[test]
    fn test_compact_summary() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let registry = Arc::new(PermissionRegistry::new(event_tx));
        let tool = LsTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "path".to_string(),
            serde_json::Value::String("/path/to/project".to_string()),
        );

        let result = "subdir/\nfile1.txt\nfile2.rs";
        assert_eq!(
            tool.compact_summary(&input, result),
            "[Ls: project (3 entries)]"
        );
    }
}
