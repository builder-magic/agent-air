//! ReadPlan tool implementation.
//!
//! Reads a specific plan by its ID and returns the full markdown content.
//!
//! Plan files are internal agent artifacts so this tool handles its own
//! permissions and never prompts the user.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::fs;

use super::plan_store::PlanStore;
use super::types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};

/// ReadPlan tool name constant.
pub const READ_PLAN_TOOL_NAME: &str = "read_plan";

/// ReadPlan tool description constant.
pub const READ_PLAN_TOOL_DESCRIPTION: &str = r#"Reads a plan by its ID and returns the full markdown content.

Usage:
- Provide the plan_id to read (e.g., "plan-001")
- Use list_plans first to discover available plan IDs

Returns:
- The full markdown content of the plan file"#;

/// ReadPlan tool JSON schema constant.
pub const READ_PLAN_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "plan_id": {
            "type": "string",
            "description": "Plan ID to read (e.g., plan-001)"
        }
    },
    "required": ["plan_id"]
}"#;

/// Tool that reads a specific plan file by ID.
///
/// Plan files live in `.agent-air/plans/` and are internal agent artifacts,
/// so no user permission is required.
pub struct ReadPlanTool {
    /// Shared plan store for directory paths.
    plan_store: Arc<PlanStore>,
}

impl ReadPlanTool {
    /// Create a new ReadPlanTool.
    pub fn new(plan_store: Arc<PlanStore>) -> Self {
        Self { plan_store }
    }
}

impl Executable for ReadPlanTool {
    fn name(&self) -> &str {
        READ_PLAN_TOOL_NAME
    }

    fn description(&self) -> &str {
        READ_PLAN_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        READ_PLAN_TOOL_SCHEMA
    }

    fn tool_type(&self) -> ToolType {
        ToolType::Custom
    }

    fn execute(
        &self,
        _context: ToolContext,
        input: HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let plan_store = self.plan_store.clone();

        Box::pin(async move {
            let plan_id = input
                .get("plan_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'plan_id' parameter".to_string())?;

            let plans_dir = plan_store.plans_dir();
            let file_path = plans_dir.join(format!("{}.md", plan_id));

            if !file_path.exists() {
                return Err(format!(
                    "Plan '{}' not found. Use list_plans to see available plans.",
                    plan_id
                ));
            }

            let content = fs::read_to_string(&file_path)
                .await
                .map_err(|e| format!("Failed to read plan file: {}", e))?;

            Ok(content)
        })
    }

    fn handles_own_permissions(&self) -> bool {
        true
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Read Plan".to_string(),
            display_title: Box::new(|input| {
                input
                    .get("plan_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string()
            }),
            display_content: Box::new(|_input, result| {
                let lines: Vec<&str> = result.lines().take(15).collect();
                let total_lines = result.lines().count();
                let truncated = total_lines > 15;
                let content = if truncated {
                    format!("{}...\n[truncated]", lines.join("\n"))
                } else {
                    lines.join("\n")
                };

                DisplayResult {
                    content,
                    content_type: ResultContentType::Markdown,
                    is_truncated: truncated,
                    full_length: total_lines,
                }
            }),
        }
    }

    fn compact_summary(&self, input: &HashMap<String, serde_json::Value>, _result: &str) -> String {
        let plan_id = input
            .get("plan_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        format!("[ReadPlan: {}]", plan_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_context(tool_use_id: &str) -> ToolContext {
        ToolContext {
            session_id: 1,
            tool_use_id: tool_use_id.to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        }
    }

    #[tokio::test]
    async fn test_read_existing_plan() {
        let temp_dir = TempDir::new().unwrap();
        let plans_dir = temp_dir.path().join(".agent-air/plans");
        fs::create_dir_all(&plans_dir).await.unwrap();

        let plan_content = "# Plan: My Plan\n\n**ID**: plan-001\n**Status**: active\n\n## Steps\n\n1. [ ] Do something\n";
        fs::write(plans_dir.join("plan-001.md"), plan_content)
            .await
            .unwrap();

        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = ReadPlanTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );

        let result = tool.execute(make_context("test-read"), input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("# Plan: My Plan"));
        assert!(output.contains("1. [ ] Do something"));
    }

    #[tokio::test]
    async fn test_read_nonexistent_plan() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = ReadPlanTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-999".to_string()),
        );

        let result = tool.execute(make_context("test-missing"), input).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("not found"));
        assert!(err.contains("list_plans"));
    }

    #[tokio::test]
    async fn test_read_missing_plan_id() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = ReadPlanTool::new(plan_store);

        let result = tool
            .execute(make_context("test-no-id"), HashMap::new())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required 'plan_id'"));
    }

    #[test]
    fn test_compact_summary() {
        let plan_store = Arc::new(PlanStore::new(std::path::PathBuf::from("/tmp")));
        let tool = ReadPlanTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );

        let summary = tool.compact_summary(&input, "# Plan content...");
        assert_eq!(summary, "[ReadPlan: plan-001]");
    }

    #[test]
    fn test_handles_own_permissions() {
        let plan_store = Arc::new(PlanStore::new(std::path::PathBuf::from("/tmp")));
        let tool = ReadPlanTool::new(plan_store);
        assert!(tool.handles_own_permissions());
    }
}
