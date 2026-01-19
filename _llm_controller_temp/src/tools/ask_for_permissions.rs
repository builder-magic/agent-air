//! AskForPermissions tool implementation
//!
//! This tool allows the LLM to request permission from the user before
//! performing sensitive actions like file writes, deletions, or network operations.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::permission_registry::PermissionRegistry;
use super::types::{DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType};

/// AskForPermissions tool name constant.
pub const ASK_FOR_PERMISSIONS_TOOL_NAME: &str = "ask_for_permissions";

/// AskForPermissions tool description constant.
pub const ASK_FOR_PERMISSIONS_TOOL_DESCRIPTION: &str =
    "Request permission from the user before performing sensitive actions like file writes, \
     deletions, network operations, or system commands. The user can grant permission for \
     this request only (once) or for the remainder of the session.";

/// AskForPermissions tool JSON schema constant.
pub const ASK_FOR_PERMISSIONS_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "action": {
            "type": "string",
            "description": "Description of the action requiring permission"
        },
        "reason": {
            "type": "string",
            "description": "Why this action is needed"
        },
        "resources": {
            "type": "array",
            "items": { "type": "string" },
            "description": "Resources affected (file paths, URLs, etc.)"
        },
        "category": {
            "type": "string",
            "enum": ["file_write", "file_delete", "network", "system", "other"],
            "description": "Category of permission being requested"
        }
    },
    "required": ["action", "category"]
}"#;

/// Categories of permissions that can be requested.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PermissionCategory {
    /// Writing to files.
    FileWrite,
    /// Deleting files.
    FileDelete,
    /// Network operations (API calls, downloads, etc.).
    Network,
    /// System commands and operations.
    System,
    /// Other actions that don't fit the above categories.
    Other,
}

impl std::fmt::Display for PermissionCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionCategory::FileWrite => write!(f, "File Write"),
            PermissionCategory::FileDelete => write!(f, "File Delete"),
            PermissionCategory::Network => write!(f, "Network"),
            PermissionCategory::System => write!(f, "System"),
            PermissionCategory::Other => write!(f, "Other"),
        }
    }
}

/// Request for permission to perform an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    /// Description of the action requiring permission.
    pub action: String,
    /// Optional reason why this action is needed.
    #[serde(default)]
    pub reason: Option<String>,
    /// Resources affected (file paths, URLs, etc.).
    #[serde(default)]
    pub resources: Vec<String>,
    /// Category of permission being requested.
    pub category: PermissionCategory,
}

impl PermissionRequest {
    /// Validate the request structure.
    pub fn validate(&self) -> Result<(), String> {
        if self.action.trim().is_empty() {
            return Err("Action description cannot be empty".to_string());
        }
        Ok(())
    }
}

/// Scope of the permission grant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionScope {
    /// Grant for this request only.
    Once,
    /// Grant for the remainder of the session.
    Session,
}

impl std::fmt::Display for PermissionScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionScope::Once => write!(f, "Once"),
            PermissionScope::Session => write!(f, "Session"),
        }
    }
}

/// Response to a permission request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionResponse {
    /// Whether permission was granted.
    pub granted: bool,
    /// Scope of the grant (if granted).
    #[serde(default)]
    pub scope: Option<PermissionScope>,
    /// Optional message from the user.
    #[serde(default)]
    pub message: Option<String>,
}

impl PermissionResponse {
    /// Create a granted response with the specified scope.
    pub fn grant(scope: PermissionScope) -> Self {
        Self {
            granted: true,
            scope: Some(scope),
            message: None,
        }
    }

    /// Create a denied response with an optional message.
    pub fn deny(message: Option<String>) -> Self {
        Self {
            granted: false,
            scope: None,
            message,
        }
    }
}

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
            // Parse the action
            let action = input
                .get("action")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing 'action' field".to_string())?
                .to_string();

            // Parse the category
            let category_str = input
                .get("category")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing 'category' field".to_string())?;

            let category: PermissionCategory = serde_json::from_value(
                serde_json::Value::String(category_str.to_string()),
            )
            .map_err(|e| format!("Invalid category '{}': {}", category_str, e))?;

            // Parse optional reason
            let reason = input
                .get("reason")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Parse optional resources
            let resources = input
                .get("resources")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let request = PermissionRequest {
                action,
                reason,
                resources,
                category,
            };

            // Validate the request
            request.validate()?;

            // Check if already granted for this session
            if registry.is_granted(context.session_id, &request).await {
                return Ok(serde_json::to_string(&PermissionResponse::grant(
                    PermissionScope::Session,
                ))
                .unwrap());
            }

            // Register the request and wait for user response
            let rx = registry
                .register(
                    context.tool_use_id,
                    context.session_id,
                    request,
                    context.turn_id,
                )
                .await
                .map_err(|e| format!("Failed to register permission request: {}", e))?;

            // Block waiting for user response
            let response = rx
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
                input
                    .get("category")
                    .and_then(|v| v.as_str())
                    .map(|s| {
                        match s {
                            "file_write" => "File Write",
                            "file_delete" => "File Delete",
                            "network" => "Network",
                            "system" => "System",
                            _ => "Permission",
                        }
                        .to_string()
                    })
                    .unwrap_or_else(|| "Permission".to_string())
            }),
            display_content: Box::new(|input, _result| {
                let action = input
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown action");

                let reason = input
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .map(|r| format!("\nReason: {}", r))
                    .unwrap_or_default();

                let resources = input
                    .get("resources")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        let items: Vec<_> = arr
                            .iter()
                            .filter_map(|v| v.as_str())
                            .collect();
                        if items.is_empty() {
                            String::new()
                        } else {
                            format!("\nResources:\n  {}", items.join("\n  "))
                        }
                    })
                    .unwrap_or_default();

                let content = format!("{}{}{}", action, reason, resources);

                DisplayResult {
                    content,
                    content_type: ResultContentType::PlainText,
                    is_truncated: false,
                    full_length: 0,
                }
            }),
        }
    }

    fn compact_summary(
        &self,
        input: &HashMap<String, serde_json::Value>,
        result: &str,
    ) -> String {
        let category = input
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let granted = result.contains("\"granted\":true");
        let status = if granted { "granted" } else { "denied" };

        format!("[Permission {}: {}]", category, status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_permission_category() {
        let file_write: PermissionCategory =
            serde_json::from_str("\"file_write\"").unwrap();
        assert_eq!(file_write, PermissionCategory::FileWrite);

        let file_delete: PermissionCategory =
            serde_json::from_str("\"file_delete\"").unwrap();
        assert_eq!(file_delete, PermissionCategory::FileDelete);

        let network: PermissionCategory = serde_json::from_str("\"network\"").unwrap();
        assert_eq!(network, PermissionCategory::Network);

        let system: PermissionCategory = serde_json::from_str("\"system\"").unwrap();
        assert_eq!(system, PermissionCategory::System);

        let other: PermissionCategory = serde_json::from_str("\"other\"").unwrap();
        assert_eq!(other, PermissionCategory::Other);
    }

    #[test]
    fn test_permission_category_display() {
        assert_eq!(format!("{}", PermissionCategory::FileWrite), "File Write");
        assert_eq!(format!("{}", PermissionCategory::FileDelete), "File Delete");
        assert_eq!(format!("{}", PermissionCategory::Network), "Network");
        assert_eq!(format!("{}", PermissionCategory::System), "System");
        assert_eq!(format!("{}", PermissionCategory::Other), "Other");
    }

    #[test]
    fn test_permission_request_validation() {
        let valid = PermissionRequest {
            action: "Delete file".to_string(),
            reason: Some("Cleanup".to_string()),
            resources: vec!["/tmp/foo.txt".to_string()],
            category: PermissionCategory::FileDelete,
        };
        assert!(valid.validate().is_ok());

        let empty_action = PermissionRequest {
            action: "   ".to_string(),
            reason: None,
            resources: vec![],
            category: PermissionCategory::Other,
        };
        assert!(empty_action.validate().is_err());
    }

    #[test]
    fn test_permission_response_grant() {
        let response = PermissionResponse::grant(PermissionScope::Session);
        assert!(response.granted);
        assert_eq!(response.scope, Some(PermissionScope::Session));
        assert!(response.message.is_none());
    }

    #[test]
    fn test_permission_response_deny() {
        let response = PermissionResponse::deny(Some("Not allowed".to_string()));
        assert!(!response.granted);
        assert!(response.scope.is_none());
        assert_eq!(response.message, Some("Not allowed".to_string()));
    }

    #[test]
    fn test_permission_scope_display() {
        assert_eq!(format!("{}", PermissionScope::Once), "Once");
        assert_eq!(format!("{}", PermissionScope::Session), "Session");
    }

    #[test]
    fn test_permission_request_serialization() {
        let request = PermissionRequest {
            action: "Write file".to_string(),
            reason: Some("Save data".to_string()),
            resources: vec!["/tmp/data.json".to_string()],
            category: PermissionCategory::FileWrite,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"action\":\"Write file\""));
        assert!(json.contains("\"category\":\"file_write\""));
        assert!(json.contains("\"resources\":["));

        let parsed: PermissionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.action, request.action);
        assert_eq!(parsed.category, request.category);
    }

    #[test]
    fn test_permission_response_serialization() {
        let response = PermissionResponse {
            granted: true,
            scope: Some(PermissionScope::Session),
            message: Some("OK".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"granted\":true"));
        assert!(json.contains("\"scope\":\"session\""));

        let parsed: PermissionResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.granted);
        assert_eq!(parsed.scope, Some(PermissionScope::Session));
    }
}
