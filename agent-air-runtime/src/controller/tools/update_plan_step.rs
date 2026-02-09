//! UpdatePlanStep tool implementation.
//!
//! Updates the status of a single step in an existing markdown plan file.
//! Finds the step by its 1-indexed number and replaces the checkbox marker
//! (`[ ]`, `[~]`, `[x]`, or `[-]`) with the requested status.
//!
//! Plan files are internal agent artifacts so this tool handles its own
//! permissions and never prompts the user.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use regex::Regex;
use tokio::fs;

use super::plan_store::PlanStore;
use super::types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};

/// UpdatePlanStep tool name constant.
pub const UPDATE_PLAN_STEP_TOOL_NAME: &str = "update_plan_step";

/// UpdatePlanStep tool description constant.
pub const UPDATE_PLAN_STEP_TOOL_DESCRIPTION: &str = r#"Updates the status of a single step in an existing plan.

Usage:
- Specify the plan_id and 1-indexed step number
- Status can be: pending, in_progress, completed, or skipped
- The plan file must already exist in .agent-air/plans/

Returns:
- The updated step line and its new status"#;

/// UpdatePlanStep tool JSON schema constant.
pub const UPDATE_PLAN_STEP_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "plan_id": {
            "type": "string",
            "description": "Plan ID"
        },
        "step": {
            "type": "integer",
            "description": "Step number (1-indexed)",
            "minimum": 1
        },
        "status": {
            "type": "string",
            "enum": ["pending", "in_progress", "completed", "skipped"]
        }
    },
    "required": ["plan_id", "step", "status"]
}"#;

/// Tool that updates the status of a single step in a plan file.
///
/// Plan files live in `.agent-air/plans/` and are internal agent artifacts,
/// so no user permission is required.
pub struct UpdatePlanStepTool {
    /// Shared plan store for directory paths and file locking.
    plan_store: Arc<PlanStore>,
}

impl UpdatePlanStepTool {
    /// Create a new UpdatePlanStepTool.
    pub fn new(plan_store: Arc<PlanStore>) -> Self {
        Self { plan_store }
    }
}

impl Executable for UpdatePlanStepTool {
    fn name(&self) -> &str {
        UPDATE_PLAN_STEP_TOOL_NAME
    }

    fn description(&self) -> &str {
        UPDATE_PLAN_STEP_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        UPDATE_PLAN_STEP_TOOL_SCHEMA
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
            // Step 1: Extract and validate parameters.
            let plan_id = input
                .get("plan_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'plan_id' parameter".to_string())?;

            let step_num = input.get("step").and_then(|v| v.as_u64()).ok_or_else(|| {
                "Missing required 'step' parameter (must be a positive integer)".to_string()
            })? as usize;

            if step_num == 0 {
                return Err("'step' must be >= 1 (1-indexed)".to_string());
            }

            let status = input
                .get("status")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'status' parameter".to_string())?;

            let new_marker = PlanStore::status_to_marker(status)?;

            // Step 2: Build file path and validate existence.
            let plans_dir = plan_store.plans_dir();
            let file_path = plans_dir.join(format!("{}.md", plan_id));
            let file_path_str = file_path.to_string_lossy().to_string();

            if !file_path.exists() {
                return Err(format!(
                    "Plan file not found: {}. Create the plan first using markdown_plan.",
                    file_path_str
                ));
            }

            // Step 3: Acquire per-file lock.
            let lock = plan_store.acquire_lock(&file_path).await;
            let _guard = lock.lock().await;

            // Step 4: Read and parse the plan file.
            let content = fs::read_to_string(&file_path)
                .await
                .map_err(|e| format!("Failed to read plan file: {}", e))?;

            // Step 5: Find and update the step line.
            // Step lines match the pattern: `N. [marker] description`
            let step_pattern = Regex::new(r"^(\d+)\. \[[ x~-]\] ")
                .map_err(|e| format!("Internal regex error: {}", e))?;
            let marker_re =
                Regex::new(r"\[[ x~-]\]").map_err(|e| format!("Internal regex error: {}", e))?;

            let mut lines: Vec<String> = content.lines().map(String::from).collect();
            let mut step_count: usize = 0;
            let mut updated_line: Option<String> = None;

            for line in &mut lines {
                if step_pattern.is_match(line) {
                    step_count += 1;
                    if step_count == step_num {
                        // Replace the marker inside the brackets.
                        let replacement = format!("[{}]", new_marker);
                        *line = marker_re
                            .replace(line.as_str(), replacement.as_str())
                            .to_string();
                        updated_line = Some(line.clone());
                        break;
                    }
                }
            }

            match updated_line {
                None => {
                    if step_count == 0 {
                        Err(format!(
                            "No steps found in plan '{}'. The plan file may be malformed.",
                            plan_id
                        ))
                    } else {
                        Err(format!(
                            "Step {} is out of range. Plan '{}' has {} step(s).",
                            step_num, plan_id, step_count
                        ))
                    }
                }
                Some(updated) => {
                    // Step 6: Write the updated content back.
                    let new_content = lines.join("\n");
                    fs::write(&file_path, &new_content)
                        .await
                        .map_err(|e| format!("Failed to write updated plan file: {}", e))?;

                    Ok(format!(
                        "Step {} updated to {}: {}",
                        step_num, status, updated
                    ))
                }
            }
        })
    }

    fn handles_own_permissions(&self) -> bool {
        true
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Update Plan Step".to_string(),
            display_title: Box::new(|input| {
                let plan_id = input
                    .get("plan_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let step = input.get("step").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("{} step {}", plan_id, step)
            }),
            display_content: Box::new(|_input, result| DisplayResult {
                content: result.to_string(),
                content_type: ResultContentType::PlainText,
                is_truncated: false,
                full_length: result.lines().count(),
            }),
        }
    }

    fn compact_summary(&self, input: &HashMap<String, serde_json::Value>, _result: &str) -> String {
        let plan_id = input
            .get("plan_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let step = input.get("step").and_then(|v| v.as_u64()).unwrap_or(0);
        let status = input
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        format!("[UpdatePlanStep: {} step {} -> {}]", plan_id, step, status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper to create a plan file with the given steps for testing.
    async fn create_test_plan(plans_dir: &std::path::Path, plan_id: &str, steps: &[&str]) {
        fs::create_dir_all(plans_dir).await.unwrap();
        let mut content = format!(
            "# Plan: Test\n\n**ID**: {}\n**Status**: active\n**Created**: 2025-01-01\n\n## Steps\n\n",
            plan_id
        );
        for (i, step) in steps.iter().enumerate() {
            content.push_str(&format!("{}. [ ] {}\n", i + 1, step));
        }
        fs::write(plans_dir.join(format!("{}.md", plan_id)), &content)
            .await
            .unwrap();
    }

    fn make_context(tool_use_id: &str) -> ToolContext {
        ToolContext {
            session_id: 1,
            tool_use_id: tool_use_id.to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        }
    }

    #[tokio::test]
    async fn test_update_step_to_completed() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let plans_dir = temp_dir.path().join(".agent-air/plans");

        create_test_plan(&plans_dir, "plan-001", &["First step", "Second step"]).await;

        let tool = UpdatePlanStepTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );
        input.insert("step".to_string(), serde_json::json!(1));
        input.insert(
            "status".to_string(),
            serde_json::Value::String("completed".to_string()),
        );

        let result = tool.execute(make_context("test-complete"), input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Step 1 updated to completed"));
        assert!(output.contains("[x]"));

        // Verify file on disk.
        let content = fs::read_to_string(plans_dir.join("plan-001.md"))
            .await
            .unwrap();
        assert!(content.contains("1. [x] First step"));
        assert!(content.contains("2. [ ] Second step"));
    }

    #[tokio::test]
    async fn test_update_step_to_in_progress() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let plans_dir = temp_dir.path().join(".agent-air/plans");

        create_test_plan(&plans_dir, "plan-001", &["Step A"]).await;

        let tool = UpdatePlanStepTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );
        input.insert("step".to_string(), serde_json::json!(1));
        input.insert(
            "status".to_string(),
            serde_json::Value::String("in_progress".to_string()),
        );

        let result = tool.execute(make_context("test-progress"), input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("[~]"));
    }

    #[tokio::test]
    async fn test_update_step_to_skipped() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let plans_dir = temp_dir.path().join(".agent-air/plans");

        create_test_plan(&plans_dir, "plan-001", &["Skip me"]).await;

        let tool = UpdatePlanStepTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );
        input.insert("step".to_string(), serde_json::json!(1));
        input.insert(
            "status".to_string(),
            serde_json::Value::String("skipped".to_string()),
        );

        let result = tool.execute(make_context("test-skip"), input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("[-]"));
    }

    #[tokio::test]
    async fn test_step_out_of_range() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let plans_dir = temp_dir.path().join(".agent-air/plans");

        create_test_plan(&plans_dir, "plan-001", &["Only step"]).await;

        let tool = UpdatePlanStepTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );
        input.insert("step".to_string(), serde_json::json!(5));
        input.insert(
            "status".to_string(),
            serde_json::Value::String("completed".to_string()),
        );

        let result = tool.execute(make_context("test-range"), input).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("out of range"));
        assert!(err.contains("1 step(s)"));
    }

    #[tokio::test]
    async fn test_plan_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));

        let tool = UpdatePlanStepTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-999".to_string()),
        );
        input.insert("step".to_string(), serde_json::json!(1));
        input.insert(
            "status".to_string(),
            serde_json::Value::String("completed".to_string()),
        );

        let result = tool.execute(make_context("test-notfound"), input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Plan file not found"));
    }

    #[tokio::test]
    async fn test_missing_required_fields() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = UpdatePlanStepTool::new(plan_store);

        // Missing plan_id.
        let mut input = HashMap::new();
        input.insert("step".to_string(), serde_json::json!(1));
        input.insert(
            "status".to_string(),
            serde_json::Value::String("completed".to_string()),
        );

        let result = tool.execute(make_context("test-missing"), input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required 'plan_id'"));

        // Missing step.
        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );
        input.insert(
            "status".to_string(),
            serde_json::Value::String("completed".to_string()),
        );

        let result = tool.execute(make_context("test-missing-step"), input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required 'step'"));

        // Missing status.
        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );
        input.insert("step".to_string(), serde_json::json!(1));

        let result = tool
            .execute(make_context("test-missing-status"), input)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required 'status'"));
    }

    #[test]
    fn test_compact_summary() {
        let plan_store = Arc::new(PlanStore::new(std::path::PathBuf::from("/tmp")));
        let tool = UpdatePlanStepTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );
        input.insert("step".to_string(), serde_json::json!(3));
        input.insert(
            "status".to_string(),
            serde_json::Value::String("completed".to_string()),
        );

        let summary = tool.compact_summary(&input, "");
        assert_eq!(summary, "[UpdatePlanStep: plan-001 step 3 -> completed]");
    }

    #[tokio::test]
    async fn test_update_already_completed_step() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let plans_dir = temp_dir.path().join(".agent-air/plans");

        // Create a plan with a completed step.
        fs::create_dir_all(&plans_dir).await.unwrap();
        let content = "# Plan: Test\n\n**ID**: plan-001\n**Status**: active\n**Created**: 2025-01-01\n\n## Steps\n\n1. [x] Already done\n";
        fs::write(plans_dir.join("plan-001.md"), content)
            .await
            .unwrap();

        let tool = UpdatePlanStepTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );
        input.insert("step".to_string(), serde_json::json!(1));
        input.insert(
            "status".to_string(),
            serde_json::Value::String("pending".to_string()),
        );

        let result = tool.execute(make_context("test-revert"), input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("[ ]"));

        // Verify on disk.
        let updated = fs::read_to_string(plans_dir.join("plan-001.md"))
            .await
            .unwrap();
        assert!(updated.contains("1. [ ] Already done"));
    }

    #[test]
    fn test_handles_own_permissions() {
        let plan_store = Arc::new(PlanStore::new(std::path::PathBuf::from("/tmp")));
        let tool = UpdatePlanStepTool::new(plan_store);
        assert!(tool.handles_own_permissions());
    }
}
