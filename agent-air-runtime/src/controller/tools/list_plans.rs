//! ListPlans tool implementation.
//!
//! Lists all plans in the `.agent-air/plans/` directory with summary
//! metadata parsed from each plan file's header: title, status, created
//! date, and step counts by status.
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

/// ListPlans tool name constant.
pub const LIST_PLANS_TOOL_NAME: &str = "list_plans";

/// ListPlans tool description constant.
pub const LIST_PLANS_TOOL_DESCRIPTION: &str = r#"Lists all plans in the workspace with summary metadata.

Usage:
- No parameters required
- Returns a summary of each plan including ID, title, status, creation date, and step progress

Returns:
- A formatted list of all plans, or a message if no plans exist"#;

/// ListPlans tool JSON schema constant.
pub const LIST_PLANS_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {},
    "required": []
}"#;

/// Tool that lists all plans in the workspace with summary metadata.
///
/// Plan files live in `.agent-air/plans/` and are internal agent artifacts,
/// so no user permission is required.
pub struct ListPlansTool {
    /// Shared plan store for directory paths.
    plan_store: Arc<PlanStore>,
}

impl ListPlansTool {
    /// Create a new ListPlansTool.
    pub fn new(plan_store: Arc<PlanStore>) -> Self {
        Self { plan_store }
    }
}

/// Parsed summary of a single plan file.
struct PlanSummary {
    plan_id: String,
    title: String,
    status: String,
    created: String,
    pending: usize,
    in_progress: usize,
    completed: usize,
    skipped: usize,
}

impl PlanSummary {
    fn total_steps(&self) -> usize {
        self.pending + self.in_progress + self.completed + self.skipped
    }
}

/// Parses a plan file's content into a `PlanSummary`.
fn parse_plan_summary(plan_id: &str, content: &str) -> PlanSummary {
    let title_re = Regex::new(r"^# Plan: (.+)$").unwrap();
    let status_re = Regex::new(r"^\*\*Status\*\*: (.+)$").unwrap();
    let created_re = Regex::new(r"^\*\*Created\*\*: (.+)$").unwrap();
    let step_re = Regex::new(r"^\d+\. \[([ x~-])\] ").unwrap();

    let mut title = String::from("Untitled");
    let mut status = String::from("unknown");
    let mut created = String::from("unknown");
    let mut pending: usize = 0;
    let mut in_progress: usize = 0;
    let mut completed: usize = 0;
    let mut skipped: usize = 0;

    for line in content.lines() {
        if let Some(caps) = title_re.captures(line) {
            title = caps[1].to_string();
        } else if let Some(caps) = status_re.captures(line) {
            status = caps[1].to_string();
        } else if let Some(caps) = created_re.captures(line) {
            created = caps[1].to_string();
        } else if let Some(caps) = step_re.captures(line) {
            match &caps[1] {
                " " => pending += 1,
                "~" => in_progress += 1,
                "x" => completed += 1,
                "-" => skipped += 1,
                _ => pending += 1,
            }
        }
    }

    PlanSummary {
        plan_id: plan_id.to_string(),
        title,
        status,
        created,
        pending,
        in_progress,
        completed,
        skipped,
    }
}

/// Formats a list of plan summaries into a readable string.
fn format_plan_list(summaries: &[PlanSummary]) -> String {
    if summaries.is_empty() {
        return "No plans found. Use markdown_plan to create one.".to_string();
    }

    let mut output = format!("Found {} plan(s):\n\n", summaries.len());

    for s in summaries {
        let total = s.total_steps();
        let progress = if total > 0 {
            format!("{}/{} completed", s.completed, total)
        } else {
            "no steps".to_string()
        };

        output.push_str(&format!(
            "- **{}**: {} [{}] (created: {}, {})\n",
            s.plan_id, s.title, s.status, s.created, progress
        ));

        // Show step breakdown if there are steps in multiple states.
        let active_states = [
            s.pending > 0,
            s.in_progress > 0,
            s.completed > 0,
            s.skipped > 0,
        ]
        .iter()
        .filter(|&&b| b)
        .count();
        if active_states > 1 {
            let mut parts = Vec::new();
            if s.completed > 0 {
                parts.push(format!("{} completed", s.completed));
            }
            if s.in_progress > 0 {
                parts.push(format!("{} in progress", s.in_progress));
            }
            if s.pending > 0 {
                parts.push(format!("{} pending", s.pending));
            }
            if s.skipped > 0 {
                parts.push(format!("{} skipped", s.skipped));
            }
            output.push_str(&format!("  Steps: {}\n", parts.join(", ")));
        }
    }

    output
}

impl Executable for ListPlansTool {
    fn name(&self) -> &str {
        LIST_PLANS_TOOL_NAME
    }

    fn description(&self) -> &str {
        LIST_PLANS_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        LIST_PLANS_TOOL_SCHEMA
    }

    fn tool_type(&self) -> ToolType {
        ToolType::Custom
    }

    fn execute(
        &self,
        _context: ToolContext,
        _input: HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let plan_store = self.plan_store.clone();

        Box::pin(async move {
            let plans_dir = plan_store.plans_dir();

            // If the directory doesn't exist, no plans have been created yet.
            if !plans_dir.exists() {
                return Ok("No plans found. Use markdown_plan to create one.".to_string());
            }

            let mut entries = fs::read_dir(plans_dir)
                .await
                .map_err(|e| format!("Failed to read plans directory: {}", e))?;

            let mut summaries = Vec::new();

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|e| format!("Failed to read directory entry: {}", e))?
            {
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();

                // Only process plan-NNN.md files.
                if let Some(plan_id) = name.strip_suffix(".md")
                    && plan_id.starts_with("plan-")
                {
                    let content = fs::read_to_string(entry.path())
                        .await
                        .map_err(|e| format!("Failed to read plan file '{}': {}", name, e))?;
                    summaries.push(parse_plan_summary(plan_id, &content));
                }
            }

            // Sort by plan ID for consistent ordering.
            summaries.sort_by(|a, b| a.plan_id.cmp(&b.plan_id));

            Ok(format_plan_list(&summaries))
        })
    }

    fn handles_own_permissions(&self) -> bool {
        true
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "List Plans".to_string(),
            display_title: Box::new(|_input| String::new()),
            display_content: Box::new(|_input, result| DisplayResult {
                content: result.to_string(),
                content_type: ResultContentType::Markdown,
                is_truncated: false,
                full_length: result.lines().count(),
            }),
        }
    }

    fn compact_summary(&self, _input: &HashMap<String, serde_json::Value>, result: &str) -> String {
        // Extract plan count from the result.
        let count = result
            .lines()
            .next()
            .and_then(|line| {
                line.strip_prefix("Found ")
                    .and_then(|s| s.split(' ').next())
                    .and_then(|n| n.parse::<usize>().ok())
            })
            .unwrap_or(0);
        format!("[ListPlans: {} plan(s)]", count)
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

    fn sample_plan(title: &str, status: &str, steps: &[&str]) -> String {
        let mut content = format!(
            "# Plan: {}\n\n**ID**: plan-001\n**Status**: {}\n**Created**: 2025-06-15\n\n## Steps\n\n",
            title, status
        );
        for (i, marker) in steps.iter().enumerate() {
            content.push_str(&format!("{}. [{}] Step {}\n", i + 1, marker, i + 1));
        }
        content
    }

    #[test]
    fn test_parse_plan_summary_basic() {
        let content = sample_plan("My Plan", "active", &[" ", "x", "~"]);
        let summary = parse_plan_summary("plan-001", &content);

        assert_eq!(summary.plan_id, "plan-001");
        assert_eq!(summary.title, "My Plan");
        assert_eq!(summary.status, "active");
        assert_eq!(summary.created, "2025-06-15");
        assert_eq!(summary.pending, 1);
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.in_progress, 1);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.total_steps(), 3);
    }

    #[test]
    fn test_parse_plan_summary_all_completed() {
        let content = sample_plan("Done", "completed", &["x", "x"]);
        let summary = parse_plan_summary("plan-002", &content);

        assert_eq!(summary.completed, 2);
        assert_eq!(summary.pending, 0);
        assert_eq!(summary.total_steps(), 2);
    }

    #[test]
    fn test_parse_plan_summary_with_skipped() {
        let content = sample_plan("Mixed", "active", &[" ", "-", "x", "-"]);
        let summary = parse_plan_summary("plan-003", &content);

        assert_eq!(summary.pending, 1);
        assert_eq!(summary.skipped, 2);
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.total_steps(), 4);
    }

    #[test]
    fn test_format_plan_list_empty() {
        let result = format_plan_list(&[]);
        assert!(result.contains("No plans found"));
    }

    #[test]
    fn test_format_plan_list_single_plan() {
        let summaries = vec![PlanSummary {
            plan_id: "plan-001".to_string(),
            title: "Test Plan".to_string(),
            status: "active".to_string(),
            created: "2025-06-15".to_string(),
            pending: 2,
            in_progress: 0,
            completed: 1,
            skipped: 0,
        }];

        let result = format_plan_list(&summaries);
        assert!(result.contains("Found 1 plan(s)"));
        assert!(result.contains("plan-001"));
        assert!(result.contains("Test Plan"));
        assert!(result.contains("[active]"));
        assert!(result.contains("1/3 completed"));
    }

    #[tokio::test]
    async fn test_list_no_plans_directory() {
        let temp_dir = TempDir::new().unwrap();
        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = ListPlansTool::new(plan_store);

        let result = tool
            .execute(make_context("test-empty"), HashMap::new())
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No plans found"));
    }

    #[tokio::test]
    async fn test_list_empty_plans_directory() {
        let temp_dir = TempDir::new().unwrap();
        let plans_dir = temp_dir.path().join(".agent-air/plans");
        fs::create_dir_all(&plans_dir).await.unwrap();

        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = ListPlansTool::new(plan_store);

        let result = tool
            .execute(make_context("test-empty-dir"), HashMap::new())
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No plans found"));
    }

    #[tokio::test]
    async fn test_list_multiple_plans() {
        let temp_dir = TempDir::new().unwrap();
        let plans_dir = temp_dir.path().join(".agent-air/plans");
        fs::create_dir_all(&plans_dir).await.unwrap();

        // Create two plan files.
        let plan1 = sample_plan("First Plan", "active", &[" ", "x"]);
        let plan2 = "# Plan: Second Plan\n\n**ID**: plan-002\n**Status**: draft\n**Created**: 2025-07-01\n\n## Steps\n\n1. [ ] Only step\n";

        fs::write(plans_dir.join("plan-001.md"), &plan1)
            .await
            .unwrap();
        fs::write(plans_dir.join("plan-002.md"), plan2)
            .await
            .unwrap();

        // Also create a non-plan file that should be ignored.
        fs::write(plans_dir.join("notes.md"), "# Notes")
            .await
            .unwrap();

        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = ListPlansTool::new(plan_store);

        let result = tool
            .execute(make_context("test-multi"), HashMap::new())
            .await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("Found 2 plan(s)"));
        assert!(output.contains("plan-001"));
        assert!(output.contains("First Plan"));
        assert!(output.contains("plan-002"));
        assert!(output.contains("Second Plan"));
        // notes.md should not appear.
        assert!(!output.contains("Notes"));
    }

    #[tokio::test]
    async fn test_list_plans_sorted_by_id() {
        let temp_dir = TempDir::new().unwrap();
        let plans_dir = temp_dir.path().join(".agent-air/plans");
        fs::create_dir_all(&plans_dir).await.unwrap();

        // Create plans out of order.
        let plan3 = "# Plan: Third\n\n**ID**: plan-003\n**Status**: draft\n**Created**: 2025-07-03\n\n## Steps\n\n1. [ ] Step\n";
        let plan1 = "# Plan: First\n\n**ID**: plan-001\n**Status**: active\n**Created**: 2025-07-01\n\n## Steps\n\n1. [x] Step\n";

        fs::write(plans_dir.join("plan-003.md"), plan3)
            .await
            .unwrap();
        fs::write(plans_dir.join("plan-001.md"), plan1)
            .await
            .unwrap();

        let plan_store = Arc::new(PlanStore::new(temp_dir.path().to_path_buf()));
        let tool = ListPlansTool::new(plan_store);

        let result = tool
            .execute(make_context("test-sort"), HashMap::new())
            .await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let pos_001 = output.find("plan-001").unwrap();
        let pos_003 = output.find("plan-003").unwrap();
        assert!(pos_001 < pos_003, "plan-001 should appear before plan-003");
    }

    #[test]
    fn test_compact_summary() {
        let plan_store = Arc::new(PlanStore::new(std::path::PathBuf::from("/tmp")));
        let tool = ListPlansTool::new(plan_store);

        let result_text = "Found 3 plan(s):\n\n- plan-001...";
        let summary = tool.compact_summary(&HashMap::new(), result_text);
        assert_eq!(summary, "[ListPlans: 3 plan(s)]");
    }

    #[test]
    fn test_compact_summary_no_plans() {
        let plan_store = Arc::new(PlanStore::new(std::path::PathBuf::from("/tmp")));
        let tool = ListPlansTool::new(plan_store);

        let result_text = "No plans found. Use markdown_plan to create one.";
        let summary = tool.compact_summary(&HashMap::new(), result_text);
        assert_eq!(summary, "[ListPlans: 0 plan(s)]");
    }

    #[test]
    fn test_handles_own_permissions() {
        let plan_store = Arc::new(PlanStore::new(std::path::PathBuf::from("/tmp")));
        let tool = ListPlansTool::new(plan_store);
        assert!(tool.handles_own_permissions());
    }
}
