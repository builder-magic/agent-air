//! MarkdownPlan tool implementation.
//!
//! Creates or updates a durable markdown plan file in `.agent-air/plans/`.
//! Each plan has a sequential ID, a title, an overall status, and numbered
//! steps with optional notes. All steps start as pending (`[ ]`).
//!
//! Plan files are internal agent artifacts so this tool handles its own
//! permissions and never prompts the user.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::Utc;
use tokio::fs;

use super::plan_store::PlanStore;
use super::types::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};

/// MarkdownPlan tool name constant.
pub const MARKDOWN_PLAN_TOOL_NAME: &str = "markdown_plan";

/// MarkdownPlan tool description constant.
pub const MARKDOWN_PLAN_TOOL_DESCRIPTION: &str = r#"Creates or updates a durable markdown plan file in the workspace.

Usage:
- Omit plan_id to create a new plan (an ID will be generated automatically)
- Provide plan_id to overwrite an existing plan
- Each step starts as pending ([ ])
- The plan file is written to .agent-air/plans/ in the workspace root

Returns:
- The plan ID, file path, and rendered markdown content"#;

/// MarkdownPlan tool JSON schema constant.
pub const MARKDOWN_PLAN_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "plan_id": {
            "type": "string",
            "description": "Plan ID to update. Omit to create a new plan."
        },
        "title": {
            "type": "string",
            "description": "Plan title"
        },
        "steps": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string"
                    },
                    "notes": {
                        "type": "string"
                    }
                },
                "required": ["description"]
            }
        },
        "status": {
            "type": "string",
            "enum": ["draft", "active", "completed", "abandoned"]
        }
    },
    "required": ["title", "steps"]
}"#;

/// Tool that creates or updates markdown plan files.
///
/// Plan files live in `.agent-air/plans/` and are internal agent artifacts,
/// so no user permission is required.
pub struct MarkdownPlanTool {
    /// Shared plan store for directory paths and file locking.
    plan_store: Arc<PlanStore>,
}

impl MarkdownPlanTool {
    /// Create a new MarkdownPlanTool.
    pub fn new(plan_store: Arc<PlanStore>) -> Self {
        Self { plan_store }
    }

    /// Renders the markdown content for a plan.
    fn generate_markdown(title: &str, plan_id: &str, status: &str, steps: &[PlanStep]) -> String {
        let date = Utc::now().format("%Y-%m-%d");
        let mut md = format!(
            "# Plan: {}\n\n**ID**: {}\n**Status**: {}\n**Created**: {}\n\n## Steps\n\n",
            title, plan_id, status, date
        );

        for (i, step) in steps.iter().enumerate() {
            md.push_str(&format!("{}. [ ] {}\n", i + 1, step.description));
            if let Some(ref notes) = step.notes {
                md.push_str(&format!("   Notes: {}\n", notes));
            }
        }

        md
    }
}

/// A single step in a plan, parsed from input.
struct PlanStep {
    description: String,
    notes: Option<String>,
}

impl Executable for MarkdownPlanTool {
    fn name(&self) -> &str {
        MARKDOWN_PLAN_TOOL_NAME
    }

    fn description(&self) -> &str {
        MARKDOWN_PLAN_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        MARKDOWN_PLAN_TOOL_SCHEMA
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
            let title = input
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'title' parameter".to_string())?;

            let steps_value = input
                .get("steps")
                .and_then(|v| v.as_array())
                .ok_or_else(|| "Missing required 'steps' parameter".to_string())?;

            if steps_value.is_empty() {
                return Err("'steps' array must not be empty".to_string());
            }

            let status = input
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("draft");

            // Parse steps from JSON array.
            let mut steps = Vec::with_capacity(steps_value.len());
            for (i, step_val) in steps_value.iter().enumerate() {
                let description = step_val
                    .get("description")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        format!("Step {} is missing required 'description' field", i + 1)
                    })?;
                let notes = step_val
                    .get("notes")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                steps.push(PlanStep {
                    description: description.to_string(),
                    notes,
                });
            }

            // Step 2: Determine plan ID.
            let plan_id = match input.get("plan_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => plan_store.get_next_plan_id().await?,
            };

            // Step 3: Build file path.
            let plans_dir = plan_store.plans_dir();
            let file_name = format!("{}.md", plan_id);
            let file_path = plans_dir.join(&file_name);
            let file_path_str = file_path.to_string_lossy().to_string();

            // Step 4: Acquire per-file lock.
            let lock = plan_store.acquire_lock(&file_path).await;
            let _guard = lock.lock().await;

            // Step 5: Ensure the plans directory exists.
            fs::create_dir_all(plans_dir)
                .await
                .map_err(|e| format!("Failed to create plans directory: {}", e))?;

            // Step 6: Generate and write the markdown.
            let markdown = Self::generate_markdown(title, &plan_id, status, &steps);
            fs::write(&file_path, &markdown)
                .await
                .map_err(|e| format!("Failed to write plan file: {}", e))?;

            Ok(format!(
                "Plan '{}' saved to {}\n\n{}",
                plan_id, file_path_str, markdown
            ))
        })
    }

    fn handles_own_permissions(&self) -> bool {
        true
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Markdown Plan".to_string(),
            display_title: Box::new(|input| {
                input
                    .get("plan_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("new plan")
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
            .unwrap_or("new");

        let step_count = input
            .get("steps")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        let status = input
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("draft");

        format!(
            "[MarkdownPlan: {} ({} steps, {})]",
            plan_id, step_count, status
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_steps(descriptions: &[&str]) -> serde_json::Value {
        let steps: Vec<serde_json::Value> = descriptions
            .iter()
            .map(|d| serde_json::json!({ "description": d }))
            .collect();
        serde_json::Value::Array(steps)
    }

    fn make_steps_with_notes(items: &[(&str, Option<&str>)]) -> serde_json::Value {
        let steps: Vec<serde_json::Value> = items
            .iter()
            .map(|(desc, notes)| {
                let mut step = serde_json::json!({ "description": desc });
                if let Some(n) = notes {
                    step.as_object_mut()
                        .unwrap()
                        .insert("notes".to_string(), serde_json::json!(n));
                }
                step
            })
            .collect();
        serde_json::Value::Array(steps)
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
    async fn test_create_new_plan() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = MarkdownPlanTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "title".to_string(),
            serde_json::Value::String("My Test Plan".to_string()),
        );
        input.insert("steps".to_string(), make_steps(&["Step one", "Step two"]));

        let result = tool.execute(make_context("test-create"), input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("plan-001"));
        assert!(output.contains("My Test Plan"));
        assert!(output.contains("1. [ ] Step one"));
        assert!(output.contains("2. [ ] Step two"));

        // Verify the file was created on disk.
        let plan_file = temp_dir.path().join(".agent-air/plans/plan-001.md");
        assert!(plan_file.exists());
    }

    #[tokio::test]
    async fn test_upsert_existing_plan() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));

        // Create the plans directory and an existing plan file.
        let plans_dir = temp_dir.path().join(".agent-air/plans");
        fs::create_dir_all(&plans_dir).await.unwrap();
        fs::write(plans_dir.join("plan-001.md"), "# Old plan")
            .await
            .unwrap();

        let tool = MarkdownPlanTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );
        input.insert(
            "title".to_string(),
            serde_json::Value::String("Updated Plan".to_string()),
        );
        input.insert("steps".to_string(), make_steps(&["New step"]));
        input.insert(
            "status".to_string(),
            serde_json::Value::String("active".to_string()),
        );

        let result = tool.execute(make_context("test-upsert"), input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("plan-001"));
        assert!(output.contains("Updated Plan"));
        assert!(output.contains("active"));
        assert!(output.contains("1. [ ] New step"));
    }

    #[tokio::test]
    async fn test_sequential_id_generation() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));

        // Create plan-001.md and plan-002.md.
        let plans_dir = temp_dir.path().join(".agent-air/plans");
        fs::create_dir_all(&plans_dir).await.unwrap();
        fs::write(plans_dir.join("plan-001.md"), "").await.unwrap();
        fs::write(plans_dir.join("plan-002.md"), "").await.unwrap();

        let next_id = plan_store.get_next_plan_id().await.unwrap();
        assert_eq!(next_id, "plan-003");
    }

    #[tokio::test]
    async fn test_missing_required_fields() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = MarkdownPlanTool::new(plan_store);

        // Missing title.
        let mut input = HashMap::new();
        input.insert("steps".to_string(), make_steps(&["A step"]));

        let result = tool.execute(make_context("test-no-title"), input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required 'title'"));

        // Missing steps.
        let mut input = HashMap::new();
        input.insert(
            "title".to_string(),
            serde_json::Value::String("A Title".to_string()),
        );

        let result = tool.execute(make_context("test-no-steps"), input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required 'steps'"));
    }

    #[tokio::test]
    async fn test_default_status_is_draft() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = MarkdownPlanTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "title".to_string(),
            serde_json::Value::String("Draft Plan".to_string()),
        );
        input.insert("steps".to_string(), make_steps(&["Step A"]));

        let result = tool.execute(make_context("test-draft"), input).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("**Status**: draft"));
    }

    #[test]
    fn test_generate_markdown_format() {
        let steps = vec![
            PlanStep {
                description: "First step".to_string(),
                notes: None,
            },
            PlanStep {
                description: "Second step".to_string(),
                notes: None,
            },
        ];

        let md = MarkdownPlanTool::generate_markdown("Test Plan", "plan-001", "draft", &steps);

        assert!(md.starts_with("# Plan: Test Plan\n"));
        assert!(md.contains("**ID**: plan-001"));
        assert!(md.contains("**Status**: draft"));
        assert!(md.contains("**Created**:"));
        assert!(md.contains("## Steps"));
        assert!(md.contains("1. [ ] First step"));
        assert!(md.contains("2. [ ] Second step"));
    }

    #[test]
    fn test_steps_with_notes() {
        let steps = vec![
            PlanStep {
                description: "Step with notes".to_string(),
                notes: Some("Important context here".to_string()),
            },
            PlanStep {
                description: "Step without notes".to_string(),
                notes: None,
            },
        ];

        let md = MarkdownPlanTool::generate_markdown("Noted Plan", "plan-005", "active", &steps);

        assert!(md.contains("1. [ ] Step with notes\n   Notes: Important context here"));
        assert!(md.contains("2. [ ] Step without notes\n"));
        // The step without notes should not have a Notes: line.
        assert!(!md.contains("2. [ ] Step without notes\n   Notes:"));
    }

    #[test]
    fn test_compact_summary() {
        let plan_store = Arc::new(PlanStore::new(std::path::PathBuf::from("/tmp")));
        let tool = MarkdownPlanTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "plan_id".to_string(),
            serde_json::Value::String("plan-001".to_string()),
        );
        input.insert("steps".to_string(), make_steps(&["A", "B", "C"]));
        input.insert(
            "status".to_string(),
            serde_json::Value::String("active".to_string()),
        );

        let summary = tool.compact_summary(&input, "");
        assert_eq!(summary, "[MarkdownPlan: plan-001 (3 steps, active)]");
    }

    #[test]
    fn test_compact_summary_defaults() {
        let plan_store = Arc::new(PlanStore::new(std::path::PathBuf::from("/tmp")));
        let tool = MarkdownPlanTool::new(plan_store);

        let input = HashMap::new();
        let summary = tool.compact_summary(&input, "");
        assert_eq!(summary, "[MarkdownPlan: new (0 steps, draft)]");
    }

    #[tokio::test]
    async fn test_empty_steps_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = MarkdownPlanTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "title".to_string(),
            serde_json::Value::String("Empty".to_string()),
        );
        input.insert("steps".to_string(), serde_json::Value::Array(vec![]));

        let result = tool.execute(make_context("test-empty"), input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not be empty"));
    }

    #[tokio::test]
    async fn test_steps_with_notes_rendered() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = MarkdownPlanTool::new(plan_store);

        let mut input = HashMap::new();
        input.insert(
            "title".to_string(),
            serde_json::Value::String("Noted".to_string()),
        );
        input.insert(
            "steps".to_string(),
            make_steps_with_notes(&[
                ("Do something", Some("Watch out for edge cases")),
                ("Do another thing", None),
            ]),
        );

        let result = tool.execute(make_context("test-notes"), input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Notes: Watch out for edge cases"));
    }

    #[test]
    fn test_handles_own_permissions() {
        let plan_store = Arc::new(PlanStore::new(std::path::PathBuf::from("/tmp")));
        let tool = MarkdownPlanTool::new(plan_store);
        assert!(tool.handles_own_permissions());
    }
}
