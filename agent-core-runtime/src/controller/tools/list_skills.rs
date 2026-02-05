//! Tool for listing available skills.

use crate::controller::tools::{
    DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType,
};
use crate::skills::SkillRegistry;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Tool name constant.
pub const LIST_SKILLS_TOOL_NAME: &str = "list_skills";

/// Tool description constant.
pub const LIST_SKILLS_TOOL_DESCRIPTION: &str = "List all available skills. Returns name, description, and SKILL.md path for each skill. Use this to discover what capabilities are available.";

/// Tool input schema (JSON Schema).
pub const LIST_SKILLS_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {},
    "additionalProperties": false
}"#;

/// Tool for listing available skills from the skill registry.
pub struct ListSkillsTool {
    skill_registry: Arc<SkillRegistry>,
}

impl ListSkillsTool {
    /// Create a new ListSkillsTool with the given skill registry.
    pub fn new(skill_registry: Arc<SkillRegistry>) -> Self {
        Self { skill_registry }
    }
}

impl Executable for ListSkillsTool {
    fn name(&self) -> &str {
        LIST_SKILLS_TOOL_NAME
    }

    fn description(&self) -> &str {
        LIST_SKILLS_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        LIST_SKILLS_TOOL_SCHEMA
    }

    fn tool_type(&self) -> ToolType {
        ToolType::Custom
    }

    fn execute(
        &self,
        _context: ToolContext,
        _input: HashMap<String, Value>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let skill_registry = self.skill_registry.clone();

        Box::pin(async move {
            let skills = skill_registry.list();

            if skills.is_empty() {
                return Ok("No skills available.".to_string());
            }

            let output: Vec<Value> = skills
                .iter()
                .map(|s| {
                    json!({
                        "name": s.metadata.name,
                        "description": s.metadata.description,
                        "skill_md_path": s.skill_md_path.display().to_string(),
                    })
                })
                .collect();

            serde_json::to_string_pretty(&output)
                .map_err(|e| format!("Failed to serialize skills: {}", e))
        })
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "List Skills".to_string(),
            display_title: Box::new(|_input| "Available Skills".to_string()),
            display_content: Box::new(|_input, result| {
                let line_count = result.lines().count();
                DisplayResult {
                    content: result.to_string(),
                    content_type: ResultContentType::Json,
                    is_truncated: false,
                    full_length: line_count,
                }
            }),
        }
    }

    fn compact_summary(&self, _input: &HashMap<String, Value>, result: &str) -> String {
        // Count skills from JSON array
        let count = serde_json::from_str::<Vec<Value>>(result)
            .map(|v| v.len())
            .unwrap_or(0);

        format!("[List Skills: {} available]", count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::{Skill, SkillMetadata};
    use std::path::PathBuf;

    fn create_test_skill(name: &str, description: &str) -> Skill {
        Skill {
            metadata: SkillMetadata {
                name: name.to_string(),
                description: description.to_string(),
                license: None,
                compatibility: None,
                metadata: None,
                allowed_tools: None,
            },
            path: PathBuf::from(format!("/skills/{}", name)),
            skill_md_path: PathBuf::from(format!("/skills/{}/SKILL.md", name)),
        }
    }

    #[tokio::test]
    async fn test_list_skills_empty() {
        let registry = Arc::new(SkillRegistry::new());
        let tool = ListSkillsTool::new(registry);

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, HashMap::new()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "No skills available.");
    }

    #[tokio::test]
    async fn test_list_skills_with_skills() {
        let registry = Arc::new(SkillRegistry::new());
        registry.register(create_test_skill("pdf-tools", "Extract text from PDFs"));
        registry.register(create_test_skill("git-helper", "Git operations helper"));

        let tool = ListSkillsTool::new(registry);

        let context = ToolContext {
            session_id: 1,
            tool_use_id: "test".to_string(),
            turn_id: None,
            permissions_pre_approved: false,
        };

        let result = tool.execute(context, HashMap::new()).await.unwrap();

        // Parse the JSON output
        let skills: Vec<Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(skills.len(), 2);

        let names: Vec<&str> = skills
            .iter()
            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
            .collect();

        assert!(names.contains(&"pdf-tools"));
        assert!(names.contains(&"git-helper"));
    }

    #[test]
    fn test_tool_metadata() {
        let registry = Arc::new(SkillRegistry::new());
        let tool = ListSkillsTool::new(registry);

        assert_eq!(tool.name(), LIST_SKILLS_TOOL_NAME);
        assert_eq!(tool.description(), LIST_SKILLS_TOOL_DESCRIPTION);
        assert!(!tool.input_schema().is_empty());
    }

    #[test]
    fn test_compact_summary() {
        let registry = Arc::new(SkillRegistry::new());
        let tool = ListSkillsTool::new(registry);

        let result = r#"[{"name":"a"},{"name":"b"},{"name":"c"}]"#;
        let summary = tool.compact_summary(&HashMap::new(), result);

        assert_eq!(summary, "[List Skills: 3 available]");
    }

    #[test]
    fn test_compact_summary_empty() {
        let registry = Arc::new(SkillRegistry::new());
        let tool = ListSkillsTool::new(registry);

        let summary = tool.compact_summary(&HashMap::new(), "No skills available.");
        assert_eq!(summary, "[List Skills: 0 available]");
    }
}
