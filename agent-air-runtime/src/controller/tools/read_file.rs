//! ReadFile tool implementation
//!
//! This tool allows the LLM to read files from the local filesystem.
//! It supports pagination via offset/limit parameters and handles
//! binary file detection. Requires permission to read files.

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use tokio::fs;
use tokio::io::AsyncReadExt;

use super::types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};
use crate::permissions::{GrantTarget, PermissionLevel, PermissionRegistry, PermissionRequest};

/// ReadFile tool name constant.
pub const READ_FILE_TOOL_NAME: &str = "read_file";

/// ReadFile tool description constant.
pub const READ_FILE_TOOL_DESCRIPTION: &str = r#"Reads a file from the local filesystem.

Usage:
- The file_path parameter must be an absolute path, not a relative path
- By default, it reads up to 2000 lines starting from the beginning of the file
- You can optionally specify a line offset and limit for reading large files in chunks
- Any lines longer than 2000 characters will be truncated
- Results are returned with line numbers starting at 1
- Binary files cannot be read and will return an error"#;

/// ReadFile tool JSON schema constant.
pub const READ_FILE_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "file_path": {
            "type": "string",
            "description": "The absolute path to the file to read"
        },
        "offset": {
            "type": "integer",
            "description": "The line number to start reading from (0-based). Defaults to 0."
        },
        "limit": {
            "type": "integer",
            "description": "The maximum number of lines to read. Defaults to 2000."
        }
    },
    "required": ["file_path"]
}"#;

/// Default number of lines to read.
const DEFAULT_READ_LIMIT: usize = 2000;

/// Maximum characters per line before truncation.
const MAX_LINE_LENGTH: usize = 2000;

/// Maximum bytes to return in output.
const MAX_BYTES: usize = 50 * 1024;

/// Binary file extensions that should be rejected.
const BINARY_EXTENSIONS: &[&str] = &[
    ".zip", ".tar", ".gz", ".tgz", ".bz2", ".xz", ".7z", ".rar", ".exe", ".dll", ".so", ".dylib",
    ".a", ".lib", ".o", ".obj", ".class", ".jar", ".war", ".pyc", ".pyo", ".wasm", ".bin", ".dat",
    ".db", ".sqlite", ".sqlite3", ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".webp",
    ".tiff", ".mp3", ".mp4", ".avi", ".mov", ".mkv", ".wav", ".flac", ".ogg", ".pdf", ".doc",
    ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".odt", ".ods", ".odp",
];

/// Tool that reads files from the filesystem.
pub struct ReadFileTool {
    permission_registry: Arc<PermissionRegistry>,
}

impl ReadFileTool {
    /// Create a new ReadFileTool instance.
    pub fn new(permission_registry: Arc<PermissionRegistry>) -> Self {
        Self {
            permission_registry,
        }
    }

    fn build_permission_request(tool_use_id: &str, path: &str) -> PermissionRequest {
        let reason = "Read file contents";

        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(path, false),
            PermissionLevel::Read,
            format!("Read file: {}", path),
        )
        .with_reason(reason)
        .with_tool(READ_FILE_TOOL_NAME)
    }
}

/// Check if a file extension indicates a binary file.
fn is_binary_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let ext_lower = format!(".{}", ext.to_lowercase());
            BINARY_EXTENSIONS.contains(&ext_lower.as_str())
        })
        .unwrap_or(false)
}

/// Check if file content appears to be binary by examining bytes.
fn is_binary_content(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    let check_size = bytes.len().min(4096);
    let sample = &bytes[..check_size];

    // Check for null bytes (strong binary indicator)
    if sample.contains(&0) {
        return true;
    }

    // Count non-printable characters
    let non_printable_count = sample
        .iter()
        .filter(|&&b| b < 9 || (b > 13 && b < 32))
        .count();

    // If more than 30% non-printable, consider it binary
    (non_printable_count as f64 / sample.len() as f64) > 0.3
}

/// Find similar files in the same directory for suggestions.
async fn find_similar_files(path: &Path) -> Vec<String> {
    let Some(dir) = path.parent() else {
        return Vec::new();
    };

    let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };

    let filename_lower = filename.to_lowercase();

    let Ok(mut entries) = fs::read_dir(dir).await else {
        return Vec::new();
    };

    let mut suggestions = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        let entry_name = entry.file_name();
        let Some(entry_str) = entry_name.to_str() else {
            continue;
        };

        let entry_lower = entry_str.to_lowercase();

        // Check if names are similar
        if (entry_lower.contains(&filename_lower) || filename_lower.contains(&entry_lower))
            && let Some(full_path) = entry.path().to_str()
        {
            suggestions.push(full_path.to_string());
        }

        if suggestions.len() >= 3 {
            break;
        }
    }

    suggestions
}

impl Executable for ReadFileTool {
    fn name(&self) -> &str {
        READ_FILE_TOOL_NAME
    }

    fn description(&self) -> &str {
        READ_FILE_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        READ_FILE_TOOL_SCHEMA
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
            // Parse file_path (required)
            let file_path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'file_path' parameter".to_string())?;

            let path = Path::new(file_path);

            // Validate absolute path
            if !path.is_absolute() {
                return Err(format!(
                    "file_path must be an absolute path, got: {}",
                    file_path
                ));
            }

            // Check if file exists
            if !path.exists() {
                let suggestions = find_similar_files(path).await;
                if suggestions.is_empty() {
                    return Err(format!("File not found: {}", file_path));
                } else {
                    return Err(format!(
                        "File not found: {}\n\nDid you mean one of these?\n{}",
                        file_path,
                        suggestions.join("\n")
                    ));
                }
            }

            // Check if it's a directory
            if path.is_dir() {
                return Err(format!(
                    "Cannot read directory: {}. Use a file listing tool instead.",
                    file_path
                ));
            }

            // Request permission if not pre-approved by batch executor
            if !context.permissions_pre_approved {
                let permission_request =
                    ReadFileTool::build_permission_request(&context.tool_use_id, file_path);
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
                        "Permission denied to read '{}': {}",
                        file_path, reason
                    ));
                }
            }

            // Check binary extension
            if is_binary_extension(path) {
                return Err(format!("Cannot read binary file: {}", file_path));
            }

            // Parse optional parameters
            let offset = input
                .get("offset")
                .and_then(|v| v.as_i64())
                .map(|v| v.max(0) as usize)
                .unwrap_or(0);

            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .map(|v| v.max(1) as usize)
                .unwrap_or(DEFAULT_READ_LIMIT);

            // Read file content
            let mut file = fs::File::open(path)
                .await
                .map_err(|e| format!("Failed to open file: {}", e))?;

            let metadata = file
                .metadata()
                .await
                .map_err(|e| format!("Failed to read file metadata: {}", e))?;

            // Check file size for binary detection (read first 4KB)
            let file_size = metadata.len() as usize;
            if file_size > 0 {
                let check_size = file_size.min(4096);
                let mut check_buffer = vec![0u8; check_size];
                file.read_exact(&mut check_buffer)
                    .await
                    .map_err(|e| format!("Failed to read file: {}", e))?;

                if is_binary_content(&check_buffer) {
                    return Err(format!("Cannot read binary file: {}", file_path));
                }
            }

            // Re-read the entire file as text
            let content = fs::read_to_string(path)
                .await
                .map_err(|e| format!("Failed to read file as text: {}", e))?;

            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();

            // Apply offset and limit
            let start = offset.min(total_lines);
            let end = (start + limit).min(total_lines);

            let mut output_lines = Vec::new();
            let mut total_bytes = 0;
            let mut truncated_by_bytes = false;

            for (idx, line) in lines[start..end].iter().enumerate() {
                let line_num = start + idx + 1; // 1-based line numbers

                // Truncate long lines
                let display_line = if line.len() > MAX_LINE_LENGTH {
                    format!("{}...", &line[..MAX_LINE_LENGTH])
                } else {
                    line.to_string()
                };

                let formatted = format!("{:05}| {}", line_num, display_line);
                let line_bytes = formatted.len() + 1; // +1 for newline

                if total_bytes + line_bytes > MAX_BYTES {
                    truncated_by_bytes = true;
                    break;
                }

                output_lines.push(formatted);
                total_bytes += line_bytes;
            }

            let last_read_line = start + output_lines.len();
            let has_more_lines = total_lines > last_read_line;

            // Build output
            let mut output = String::from("<file>\n");
            output.push_str(&output_lines.join("\n"));

            if truncated_by_bytes {
                output.push_str(&format!(
                    "\n\n(Output truncated at {} bytes. Use 'offset' parameter to read beyond line {})",
                    MAX_BYTES, last_read_line
                ));
            } else if has_more_lines {
                output.push_str(&format!(
                    "\n\n(File has {} total lines. Use 'offset' parameter to read beyond line {})",
                    total_lines, last_read_line
                ));
            } else {
                output.push_str(&format!("\n\n(End of file - {} total lines)", total_lines));
            }

            output.push_str("\n</file>");

            Ok(output)
        })
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Read File".to_string(),
            display_title: Box::new(|input| {
                input
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .map(|p| {
                        // Show just the filename for the title
                        Path::new(p)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(p)
                            .to_string()
                    })
                    .unwrap_or_default()
            }),
            display_content: Box::new(|_input, result| {
                // Extract content between <file> tags for preview
                let content = result
                    .strip_prefix("<file>\n")
                    .and_then(|s| s.split("\n\n(").next())
                    .unwrap_or(result);

                let lines: Vec<&str> = content.lines().take(20).collect();
                let preview = lines.join("\n");
                let is_truncated = content.lines().count() > 20;

                DisplayResult {
                    content: preview,
                    content_type: ResultContentType::PlainText,
                    is_truncated,
                    full_length: content.lines().count(),
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

        let truncated = result.contains("Use 'offset' parameter");
        let status = if truncated { "partial" } else { "complete" };

        format!("[ReadFile: {} ({})]", filename, status)
    }

    fn required_permissions(
        &self,
        context: &ToolContext,
        input: &HashMap<String, serde_json::Value>,
    ) -> Option<Vec<PermissionRequest>> {
        // Extract file_path from input
        let file_path = input.get("file_path").and_then(|v| v.as_str())?;

        let path = Path::new(file_path);

        // Only request permission for absolute paths
        if !path.is_absolute() {
            return None;
        }

        // Build permission request using the existing helper
        let permission_request =
            ReadFileTool::build_permission_request(&context.tool_use_id, file_path);

        Some(vec![permission_request])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tokio::sync::mpsc;

    use crate::controller::types::ControllerEvent;
    use crate::permissions::{Grant, PermissionPanelResponse, PermissionRegistry};

    fn create_test_registry() -> (Arc<PermissionRegistry>, mpsc::Receiver<ControllerEvent>) {
        let (event_tx, event_rx) = mpsc::channel(10);
        (Arc::new(PermissionRegistry::new(event_tx)), event_rx)
    }

    /// Create a session-level grant response from a permission request
    fn create_session_grant(request: &PermissionRequest) -> PermissionPanelResponse {
        PermissionPanelResponse {
            granted: true,
            grant: Some(Grant::new(request.target.clone(), request.required_level)),
            message: None,
        }
    }

    #[test]
    fn test_is_binary_extension() {
        assert!(is_binary_extension(Path::new("/tmp/file.zip")));
        assert!(is_binary_extension(Path::new("/tmp/file.exe")));
        assert!(is_binary_extension(Path::new("/tmp/file.png")));
        assert!(is_binary_extension(Path::new("/tmp/file.PDF"))); // Case insensitive
        assert!(!is_binary_extension(Path::new("/tmp/file.rs")));
        assert!(!is_binary_extension(Path::new("/tmp/file.txt")));
        assert!(!is_binary_extension(Path::new("/tmp/file.json")));
    }

    #[test]
    fn test_is_binary_content() {
        // Text content
        assert!(!is_binary_content(b"Hello, world!\nThis is text."));
        assert!(!is_binary_content(b""));

        // Binary content with null bytes
        assert!(is_binary_content(&[0x00, 0x01, 0x02, 0x03]));

        // Mixed content with high ratio of non-printable
        let binary_like: Vec<u8> = (0..100).map(|i| if i % 2 == 0 { 1 } else { 65 }).collect();
        assert!(is_binary_content(&binary_like));
    }

    #[tokio::test]
    async fn test_read_file_success() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "Line 1").unwrap();
        writeln!(temp_file, "Line 2").unwrap();
        writeln!(temp_file, "Line 3").unwrap();

        let (registry, mut _event_rx) = create_test_registry();
        let tool = ReadFileTool::new(registry.clone());

        let file_path = temp_file.path().to_str().unwrap().to_string();

        // Pre-grant session permission for the file
        let permission_request = ReadFileTool::build_permission_request("pre_grant", &file_path);
        let rx = registry
            .request_permission(1, permission_request.clone(), None)
            .await
            .unwrap();
        registry
            .respond_to_request("pre_grant", create_session_grant(&permission_request))
            .await
            .unwrap();
        let _ = rx.await;

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path),
        );

        let result = tool.execute(context, input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("<file>"));
        assert!(output.contains("00001| Line 1"));
        assert!(output.contains("00002| Line 2"));
        assert!(output.contains("00003| Line 3"));
        assert!(output.contains("</file>"));
    }

    #[tokio::test]
    async fn test_read_file_with_offset() {
        let mut temp_file = NamedTempFile::new().unwrap();
        for i in 1..=10 {
            writeln!(temp_file, "Line {}", i).unwrap();
        }

        let (registry, mut _event_rx) = create_test_registry();
        let tool = ReadFileTool::new(registry.clone());

        let file_path = temp_file.path().to_str().unwrap().to_string();

        // Pre-grant session permission
        let permission_request = ReadFileTool::build_permission_request("pre_grant", &file_path);
        let rx = registry
            .request_permission(1, permission_request.clone(), None)
            .await
            .unwrap();
        registry
            .respond_to_request("pre_grant", create_session_grant(&permission_request))
            .await
            .unwrap();
        let _ = rx.await;

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path),
        );
        input.insert("offset".to_string(), serde_json::Value::Number(5.into()));
        input.insert("limit".to_string(), serde_json::Value::Number(3.into()));

        let result = tool.execute(context, input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("00006| Line 6"));
        assert!(output.contains("00007| Line 7"));
        assert!(output.contains("00008| Line 8"));
        assert!(!output.contains("00005| Line 5"));
        assert!(!output.contains("00009| Line 9"));
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let (registry, _event_rx) = create_test_registry();
        let tool = ReadFileTool::new(registry);
        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String("/nonexistent/path/file.txt".to_string()),
        );

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("File not found"));
    }

    #[tokio::test]
    async fn test_read_file_relative_path_rejected() {
        let (registry, _event_rx) = create_test_registry();
        let tool = ReadFileTool::new(registry);
        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String("relative/path/file.txt".to_string()),
        );

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be an absolute path"));
    }

    #[tokio::test]
    async fn test_read_binary_extension_rejected() {
        let (registry, mut _event_rx) = create_test_registry();
        let tool = ReadFileTool::new(registry.clone());
        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        // Create a temp file with binary extension
        let temp_dir = tempfile::tempdir().unwrap();
        let binary_path = temp_dir.path().join("test.exe");
        std::fs::write(&binary_path, b"fake binary").unwrap();

        let file_path = binary_path.to_str().unwrap().to_string();

        // Pre-grant session permission
        let permission_request = ReadFileTool::build_permission_request("pre_grant", &file_path);
        let rx = registry
            .request_permission(1, permission_request.clone(), None)
            .await
            .unwrap();
        registry
            .respond_to_request("pre_grant", create_session_grant(&permission_request))
            .await
            .unwrap();
        let _ = rx.await;

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path),
        );

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot read binary file"));
    }

    #[test]
    fn test_compact_summary() {
        let (registry, _event_rx) = create_test_registry();
        let tool = ReadFileTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String("/path/to/file.rs".to_string()),
        );

        let complete_result = "<file>\n00001| code\n\n(End of file - 1 total lines)\n</file>";
        assert_eq!(
            tool.compact_summary(&input, complete_result),
            "[ReadFile: file.rs (complete)]"
        );

        let partial_result =
            "<file>\n00001| code\n\n(Use 'offset' parameter to read beyond line 2000)\n</file>";
        assert_eq!(
            tool.compact_summary(&input, partial_result),
            "[ReadFile: file.rs (partial)]"
        );
    }

    #[test]
    fn test_build_permission_request() {
        let request =
            ReadFileTool::build_permission_request("test-id", "/home/user/project/file.rs");
        assert_eq!(request.description, "Read file: /home/user/project/file.rs");
        assert_eq!(request.reason, Some("Read file contents".to_string()));
        assert_eq!(
            request.target,
            GrantTarget::path("/home/user/project/file.rs", false)
        );
        assert_eq!(request.required_level, PermissionLevel::Read);
    }
}
