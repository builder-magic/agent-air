//! AskForPermissions tool implementation
//!
//! This tool allows the LLM to request permission from the user before
//! performing sensitive actions like file writes, deletions, or network operations.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use super::types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};
use crate::permissions::{GrantTarget, PermissionLevel, PermissionRegistry, PermissionRequest};

/// AskForPermissions tool name constant.
pub const ASK_FOR_PERMISSIONS_TOOL_NAME: &str = "ask_for_permissions";

/// AskForPermissions tool description constant.
pub const ASK_FOR_PERMISSIONS_TOOL_DESCRIPTION: &str = "Request permission from the user before performing sensitive actions like file writes, \
     deletions, network operations, or system commands. The user can grant permission for \
     this request only (once) or for the remainder of the session.";

/// AskForPermissions tool JSON schema constant.
///
/// Uses the standardized permission format with explicit target type and level.
pub const ASK_FOR_PERMISSIONS_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "target_type": {
            "type": "string",
            "enum": ["path", "domain", "command"],
            "description": "Type of resource: 'path' for files/directories, 'domain' for network endpoints, 'command' for shell commands"
        },
        "target": {
            "type": "string",
            "description": "The resource being accessed: file path, domain pattern, or command pattern"
        },
        "level": {
            "type": "string",
            "enum": ["read", "write", "execute", "admin"],
            "description": "Permission level required: 'read' for viewing, 'write' for modification, 'execute' for running, 'admin' for deletion/full control"
        },
        "recursive": {
            "type": "boolean",
            "description": "For path targets only: whether to include subdirectories",
            "default": false
        },
        "description": {
            "type": "string",
            "description": "Human-readable description of the action requiring permission"
        },
        "reason": {
            "type": "string",
            "description": "Why this action is needed"
        }
    },
    "required": ["target_type", "target", "level", "description"]
}"#;

/// Tool that requests permission from the user for sensitive actions.
pub struct AskForPermissionsTool {
    /// Registry for managing pending permission requests.
    registry: Arc<PermissionRegistry>,
}

impl AskForPermissionsTool {
    /// Create a new AskForPermissionsTool instance.
    ///
    /// # Arguments
    /// * `registry` - The permission registry to use for tracking requests.
    pub fn new(registry: Arc<PermissionRegistry>) -> Self {
        Self { registry }
    }
}

impl Executable for AskForPermissionsTool {
    fn name(&self) -> &str {
        ASK_FOR_PERMISSIONS_TOOL_NAME
    }

    fn description(&self) -> &str {
        ASK_FOR_PERMISSIONS_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        ASK_FOR_PERMISSIONS_TOOL_SCHEMA
    }

    fn tool_type(&self) -> ToolType {
        ToolType::UserInteraction
    }

    fn execute(
        &self,
        context: ToolContext,
        input: HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let registry = self.registry.clone();

        Box::pin(async move {
            // Parse target_type
            let target_type = input
                .get("target_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing 'target_type' field".to_string())?;

            // Parse target
            let target_value = input
                .get("target")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing 'target' field".to_string())?
                .to_string();

            // Parse level
            let level_str = input
                .get("level")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing 'level' field".to_string())?;

            let level = match level_str {
                "read" => PermissionLevel::Read,
                "write" => PermissionLevel::Write,
                "execute" => PermissionLevel::Execute,
                "admin" => PermissionLevel::Admin,
                _ => {
                    return Err(format!(
                        "Invalid level '{}': must be read, write, execute, or admin",
                        level_str
                    ));
                }
            };

            // Parse recursive (optional, defaults to false)
            let recursive = input
                .get("recursive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            // Parse description
            let description = input
                .get("description")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing 'description' field".to_string())?
                .to_string();

            // Validate description is not empty
            if description.trim().is_empty() {
                return Err("Description cannot be empty".to_string());
            }

            // Parse optional reason
            let reason = input
                .get("reason")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Build the grant target based on target_type
            let target = match target_type {
                "path" => GrantTarget::path(&target_value, recursive),
                "domain" => GrantTarget::Domain {
                    pattern: target_value,
                },
                "command" => GrantTarget::Command {
                    pattern: target_value,
                },
                _ => {
                    return Err(format!(
                        "Invalid target_type '{}': must be path, domain, or command",
                        target_type
                    ));
                }
            };

            // Build the permission request using the new types directly
            let mut request =
                PermissionRequest::new(&context.tool_use_id, target, level, &description);
            if let Some(r) = reason {
                request = request.with_reason(&r);
            }
            request = request.with_tool(ASK_FOR_PERMISSIONS_TOOL_NAME);

            // Request permission (auto-approves if already granted)
            let response_rx = registry
                .request_permission(context.session_id, request, context.turn_id)
                .await
                .map_err(|e| format!("Failed to request permission: {}", e))?;

            // Block waiting for user response
            let response = response_rx
                .await
                .map_err(|_| "User declined to grant permission".to_string())?;

            // Return the response as JSON
            serde_json::to_string(&response)
                .map_err(|e| format!("Failed to serialize response: {}", e))
        })
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Permission Request".to_string(),
            display_title: Box::new(|input| {
                let target_type = input
                    .get("target_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let level = input
                    .get("level")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                format!(
                    "{} {}",
                    capitalize(level),
                    match target_type {
                        "path" => "Path",
                        "domain" => "Domain",
                        "command" => "Command",
                        _ => "Permission",
                    }
                )
            }),
            display_content: Box::new(|input, _result| {
                let description = input
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown action");

                let target = input
                    .get("target")
                    .and_then(|v| v.as_str())
                    .map(|t| format!("\nTarget: {}", t))
                    .unwrap_or_default();

                let reason = input
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .map(|r| format!("\nReason: {}", r))
                    .unwrap_or_default();

                let content = format!("{}{}{}", description, target, reason);

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
        let target_type = input
            .get("target_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let level = input
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let granted = result.contains("\"granted\":true");
        let status = if granted { "granted" } else { "denied" };

        format!("[Permission {} {} {}]", level, target_type, status)
    }

    fn handles_own_permissions(&self) -> bool {
        true // This tool explicitly handles permission requests
    }
}

/// Capitalize the first letter of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_has_required_fields() {
        let schema: serde_json::Value =
            serde_json::from_str(ASK_FOR_PERMISSIONS_TOOL_SCHEMA).unwrap();

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::Value::String("target_type".to_string())));
        assert!(required.contains(&serde_json::Value::String("target".to_string())));
        assert!(required.contains(&serde_json::Value::String("level".to_string())));
        assert!(required.contains(&serde_json::Value::String("description".to_string())));
    }

    #[test]
    fn test_schema_target_type_enum() {
        let schema: serde_json::Value =
            serde_json::from_str(ASK_FOR_PERMISSIONS_TOOL_SCHEMA).unwrap();

        let target_type_enum = schema
            .get("properties")
            .unwrap()
            .get("target_type")
            .unwrap()
            .get("enum")
            .unwrap()
            .as_array()
            .unwrap();

        assert!(target_type_enum.contains(&serde_json::Value::String("path".to_string())));
        assert!(target_type_enum.contains(&serde_json::Value::String("domain".to_string())));
        assert!(target_type_enum.contains(&serde_json::Value::String("command".to_string())));
    }

    #[test]
    fn test_schema_level_enum() {
        let schema: serde_json::Value =
            serde_json::from_str(ASK_FOR_PERMISSIONS_TOOL_SCHEMA).unwrap();

        let level_enum = schema
            .get("properties")
            .unwrap()
            .get("level")
            .unwrap()
            .get("enum")
            .unwrap()
            .as_array()
            .unwrap();

        assert!(level_enum.contains(&serde_json::Value::String("read".to_string())));
        assert!(level_enum.contains(&serde_json::Value::String("write".to_string())));
        assert!(level_enum.contains(&serde_json::Value::String("execute".to_string())));
        assert!(level_enum.contains(&serde_json::Value::String("admin".to_string())));
    }

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("read"), "Read");
        assert_eq!(capitalize("write"), "Write");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("ADMIN"), "ADMIN");
    }

    #[test]
    fn test_compact_summary_format() {
        let tool = AskForPermissionsTool::new(Arc::new(PermissionRegistry::new(
            tokio::sync::mpsc::channel(1).0,
        )));

        let mut input = HashMap::new();
        input.insert("target_type".to_string(), serde_json::json!("path"));
        input.insert("level".to_string(), serde_json::json!("write"));

        let summary = tool.compact_summary(&input, r#"{"granted":true}"#);
        assert_eq!(summary, "[Permission write path granted]");

        let summary = tool.compact_summary(&input, r#"{"granted":false}"#);
        assert_eq!(summary, "[Permission write path denied]");
    }
}
