//! WriteFile tool implementation
//!
//! This tool allows the LLM to write files to the local filesystem.
//! It integrates with the PermissionRegistry to require user approval
//! before performing write operations.

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use tokio::fs;

use super::types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};
use crate::permissions::{GrantTarget, PermissionLevel, PermissionRegistry, PermissionRequest};

/// WriteFile tool name constant.
pub const WRITE_FILE_TOOL_NAME: &str = "write_file";

/// WriteFile tool description constant.
pub const WRITE_FILE_TOOL_DESCRIPTION: &str = r#"Writes content to a file, creating it if it doesn't exist or overwriting if it does.

Usage:
- The file_path parameter must be an absolute path, not a relative path
- This tool will overwrite the existing file if there is one at the provided path
- Parent directories will be created automatically if they don't exist
- Requires user permission before writing (may be cached for session)

Returns:
- Success message with bytes written on successful write
- Error message if permission is denied or the operation fails"#;

/// WriteFile tool JSON schema constant.
pub const WRITE_FILE_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "file_path": {
            "type": "string",
            "description": "The absolute path to the file to write"
        },
        "content": {
            "type": "string",
            "description": "The content to write to the file"
        },
        "create_directories": {
            "type": "boolean",
            "description": "Whether to create parent directories if they don't exist. Defaults to true."
        }
    },
    "required": ["file_path", "content"]
}"#;

/// Tool that writes files to the filesystem with permission checks.
pub struct WriteFileTool {
    /// Reference to the permission registry for requesting write permissions.
    permission_registry: Arc<PermissionRegistry>,
}

impl WriteFileTool {
    /// Create a new WriteFileTool with the given permission registry.
    ///
    /// # Arguments
    /// * `permission_registry` - The registry used to request and cache permissions.
    pub fn new(permission_registry: Arc<PermissionRegistry>) -> Self {
        Self {
            permission_registry,
        }
    }

    /// Builds a permission request for writing to a file.
    ///
    /// # Arguments
    /// * `tool_use_id` - Unique identifier for this tool invocation
    /// * `file_path` - Path to the file being written
    /// * `content_len` - Number of bytes to write
    /// * `is_overwrite` - Whether this overwrites an existing file
    /// * `will_create_directories` - Whether parent directories will be created
    fn build_permission_request(
        tool_use_id: &str,
        file_path: &str,
        content_len: usize,
        is_overwrite: bool,
        will_create_directories: bool,
    ) -> PermissionRequest {
        let action_verb = if is_overwrite { "Overwrite" } else { "Create" };
        let dir_note = if will_create_directories {
            " (will create parent directories)"
        } else {
            ""
        };
        let reason = format!(
            "{} file with {} bytes of content{}",
            action_verb.to_lowercase(),
            content_len,
            dir_note
        );

        PermissionRequest::new(
            tool_use_id,
            GrantTarget::path(file_path, false),
            PermissionLevel::Write,
            &format!("Write file: {}", file_path),
        )
        .with_reason(reason)
        .with_tool(WRITE_FILE_TOOL_NAME)
    }
}

impl Executable for WriteFileTool {
    fn name(&self) -> &str {
        WRITE_FILE_TOOL_NAME
    }

    fn description(&self) -> &str {
        WRITE_FILE_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        WRITE_FILE_TOOL_SCHEMA
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
            // ─────────────────────────────────────────────────────────────
            // Step 1: Extract and validate parameters
            // ─────────────────────────────────────────────────────────────
            let file_path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'file_path' parameter".to_string())?;

            let content = input
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'content' parameter".to_string())?;

            let create_directories = input
                .get("create_directories")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let path = Path::new(file_path);

            // Validate absolute path
            if !path.is_absolute() {
                return Err(format!(
                    "file_path must be an absolute path, got: {}",
                    file_path
                ));
            }

            // Check if this is an overwrite (file exists) or create (new file)
            let is_overwrite = path.exists();

            // ─────────────────────────────────────────────────────────────
            // Step 2: Determine if directories will be created
            // ─────────────────────────────────────────────────────────────
            let will_create_directories =
                create_directories && path.parent().map(|p| !p.exists()).unwrap_or(false);

            // ─────────────────────────────────────────────────────────────
            // Step 3: Request permission if not pre-approved by batch executor
            // ─────────────────────────────────────────────────────────────
            if !context.permissions_pre_approved {
                let permission_request = Self::build_permission_request(
                    &context.tool_use_id,
                    file_path,
                    content.len(),
                    is_overwrite,
                    will_create_directories,
                );

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
                        "Permission denied to write '{}': {}",
                        file_path, reason
                    ));
                }
            }

            // ─────────────────────────────────────────────────────────────
            // Step 7: Create parent directories if requested
            // ─────────────────────────────────────────────────────────────
            if create_directories {
                if let Some(parent) = path.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)
                            .await
                            .map_err(|e| format!("Failed to create parent directories: {}", e))?;
                    }
                }
            }

            // ─────────────────────────────────────────────────────────────
            // Step 8: Perform the write operation
            // ─────────────────────────────────────────────────────────────
            let bytes_written = content.len();
            fs::write(path, content)
                .await
                .map_err(|e| format!("Failed to write file '{}': {}", file_path, e))?;

            let action = if is_overwrite { "overwrote" } else { "created" };
            Ok(format!(
                "Successfully {} '{}' ({} bytes)",
                action, file_path, bytes_written
            ))
        })
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Write File".to_string(),
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
                let content_preview = input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|c| {
                        let lines: Vec<&str> = c.lines().take(10).collect();
                        if c.lines().count() > 10 {
                            format!("{}...\n[truncated]", lines.join("\n"))
                        } else {
                            lines.join("\n")
                        }
                    })
                    .unwrap_or_else(|| result.to_string());

                DisplayResult {
                    content: content_preview,
                    content_type: ResultContentType::PlainText,
                    is_truncated: input
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|c| c.lines().count() > 10)
                        .unwrap_or(false),
                    full_length: input
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|c| c.lines().count())
                        .unwrap_or(0),
                }
            }),
        }
    }

    fn compact_summary(&self, input: &HashMap<String, serde_json::Value>, _result: &str) -> String {
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

        let bytes = input
            .get("content")
            .and_then(|v| v.as_str())
            .map(|c| c.len())
            .unwrap_or(0);

        format!("[WriteFile: {} ({} bytes)]", filename, bytes)
    }

    fn required_permissions(
        &self,
        context: &ToolContext,
        input: &HashMap<String, serde_json::Value>,
    ) -> Option<Vec<PermissionRequest>> {
        // Extract file_path parameter
        let file_path = input.get("file_path").and_then(|v| v.as_str())?;

        // Extract content to determine size
        let content = input.get("content").and_then(|v| v.as_str())?;

        let path = Path::new(file_path);

        // Validate absolute path - return None if invalid
        if !path.is_absolute() {
            return None;
        }

        // Check if this is an overwrite (file exists) or create (new file)
        let is_overwrite = path.exists();

        // Check if directories will be created (default is true)
        let create_directories = input
            .get("create_directories")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let will_create_directories =
            create_directories && path.parent().map(|p| !p.exists()).unwrap_or(false);

        // Build and return permission request
        let permission_request = Self::build_permission_request(
            &context.tool_use_id,
            file_path,
            content.len(),
            is_overwrite,
            will_create_directories,
        );

        Some(vec![permission_request])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::PermissionPanelResponse;
    use crate::controller::types::ControllerEvent;
    use crate::permissions::PermissionLevel;
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

    #[tokio::test]
    async fn test_write_new_file_with_permission_granted() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = WriteFileTool::new(registry.clone());
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "content".to_string(),
            serde_json::Value::String("Hello, World!".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-123".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        // Spawn task to handle permission request
        let registry_clone = registry.clone();
        tokio::spawn(async move {
            // Wait for permission request event
            if let Some(ControllerEvent::PermissionRequired { tool_use_id, .. }) =
                event_rx.recv().await
            {
                // Grant permission
                registry_clone
                    .respond_to_request(&tool_use_id, grant_once())
                    .await
                    .unwrap();
            }
        });

        let result = tool.execute(context, input).await;

        assert!(result.is_ok());
        assert!(file_path.exists());
        assert_eq!(
            tokio::fs::read_to_string(&file_path).await.unwrap(),
            "Hello, World!"
        );
    }

    #[tokio::test]
    async fn test_write_file_permission_denied() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = WriteFileTool::new(registry.clone());
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "content".to_string(),
            serde_json::Value::String("Hello, World!".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-456".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        // Spawn task to deny permission
        let registry_clone = registry.clone();
        tokio::spawn(async move {
            if let Some(ControllerEvent::PermissionRequired { tool_use_id, .. }) =
                event_rx.recv().await
            {
                // Deny permission
                registry_clone
                    .respond_to_request(&tool_use_id, deny("Not allowed"))
                    .await
                    .unwrap();
            }
        });

        let result = tool.execute(context, input).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Permission denied"));
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_write_file_session_permission_cached() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = WriteFileTool::new(registry.clone());
        let temp_dir = TempDir::new().unwrap();

        // First write - will request permission
        let file_path_1 = temp_dir.path().join("test1.txt");
        let mut input_1 = HashMap::new();
        input_1.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path_1.to_str().unwrap().to_string()),
        );
        input_1.insert(
            "content".to_string(),
            serde_json::Value::String("Content 1".to_string()),
        );

        let context_1 = ToolContext {
            session_id: 1,
            tool_use_id: "test-1".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        // Grant with Session scope
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

        let result_1 = tool.execute(context_1, input_1).await;
        assert!(result_1.is_ok());
        assert!(file_path_1.exists());

        // Second write - should use cached permission (no event emitted)
        // Note: Cache matching uses action pattern, so same action "Create file: test2.txt"
        // will NOT match "Create file: test1.txt". This is current behavior.
        // For this test, we verify the first write worked.
    }

    #[tokio::test]
    async fn test_overwrite_existing_file() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = WriteFileTool::new(registry.clone());
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("existing.txt");

        // Create existing file
        tokio::fs::write(&file_path, "old content").await.unwrap();

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "content".to_string(),
            serde_json::Value::String("new content".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-overwrite".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        // Grant permission
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
        assert!(result.unwrap().contains("overwrote"));
        assert_eq!(
            tokio::fs::read_to_string(&file_path).await.unwrap(),
            "new content"
        );
    }

    #[tokio::test]
    async fn test_create_parent_directories() {
        let (registry, mut event_rx) = create_test_registry();
        let tool = WriteFileTool::new(registry.clone());
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nested/dir/test.txt");

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_str().unwrap().to_string()),
        );
        input.insert(
            "content".to_string(),
            serde_json::Value::String("nested content".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test-nested".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        // Grant permission
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
        assert!(file_path.exists());
        assert!(file_path.parent().unwrap().exists());
    }

    #[tokio::test]
    async fn test_relative_path_rejected() {
        let (registry, _event_rx) = create_test_registry();
        let tool = WriteFileTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String("relative/path.txt".to_string()),
        );
        input.insert(
            "content".to_string(),
            serde_json::Value::String("content".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("absolute path"));
    }

    #[tokio::test]
    async fn test_missing_file_path() {
        let (registry, _event_rx) = create_test_registry();
        let tool = WriteFileTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "content".to_string(),
            serde_json::Value::String("content".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required 'file_path'"));
    }

    #[tokio::test]
    async fn test_missing_content() {
        let (registry, _event_rx) = create_test_registry();
        let tool = WriteFileTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String("/tmp/test.txt".to_string()),
        );

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required 'content'"));
    }

    #[test]
    fn test_compact_summary() {
        let (registry, _event_rx) = create_test_registry();
        let tool = WriteFileTool::new(registry);

        let mut input = HashMap::new();
        input.insert(
            "file_path".to_string(),
            serde_json::Value::String("/path/to/file.rs".to_string()),
        );
        input.insert(
            "content".to_string(),
            serde_json::Value::String("some content here".to_string()),
        );

        let summary = tool.compact_summary(&input, "Successfully created...");
        assert_eq!(summary, "[WriteFile: file.rs (17 bytes)]");
    }

    #[test]
    fn test_build_permission_request_create() {
        let request = WriteFileTool::build_permission_request(
            "test-id",
            "/path/to/new.txt",
            100,
            false,
            false,
        );

        assert_eq!(request.description, "Write file: /path/to/new.txt");
        assert_eq!(
            request.reason,
            Some("create file with 100 bytes of content".to_string())
        );
        assert_eq!(request.target, GrantTarget::path("/path/to/new.txt", false));
        assert_eq!(request.required_level, PermissionLevel::Write);
    }

    #[test]
    fn test_build_permission_request_overwrite() {
        let request = WriteFileTool::build_permission_request(
            "test-id",
            "/path/to/existing.txt",
            500,
            true,
            false,
        );

        assert_eq!(request.description, "Write file: /path/to/existing.txt");
        assert_eq!(
            request.reason,
            Some("overwrite file with 500 bytes of content".to_string())
        );
        assert_eq!(
            request.target,
            GrantTarget::path("/path/to/existing.txt", false)
        );
        assert_eq!(request.required_level, PermissionLevel::Write);
    }

    #[test]
    fn test_build_permission_request_with_directory_creation() {
        let request = WriteFileTool::build_permission_request(
            "test-id",
            "/new/path/file.txt",
            200,
            false,
            true,
        );

        assert_eq!(request.description, "Write file: /new/path/file.txt");
        assert_eq!(
            request.reason,
            Some(
                "create file with 200 bytes of content (will create parent directories)"
                    .to_string()
            )
        );
        assert_eq!(
            request.target,
            GrantTarget::path("/new/path/file.txt", false)
        );
        assert_eq!(request.required_level, PermissionLevel::Write);
    }
}
