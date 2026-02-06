//! Thread-safe registry for discovered skills.

use crate::skills::types::Skill;
use std::collections::HashMap;
use std::sync::RwLock;

/// Thread-safe registry that stores discovered skills.
#[derive(Debug, Default)]
pub struct SkillRegistry {
    skills: RwLock<HashMap<String, Skill>>,
}

impl SkillRegistry {
    /// Create a new empty skill registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a skill in the registry.
    ///
    /// If a skill with the same name already exists, it will be replaced.
    pub fn register(&self, skill: Skill) -> Option<Skill> {
        let mut skills = self.skills.write().unwrap();
        skills.insert(skill.metadata.name.clone(), skill)
    }

    /// Unregister a skill by name.
    ///
    /// Returns the removed skill if it existed.
    pub fn unregister(&self, name: &str) -> Option<Skill> {
        let mut skills = self.skills.write().unwrap();
        skills.remove(name)
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<Skill> {
        let skills = self.skills.read().unwrap();
        skills.get(name).cloned()
    }

    /// List all registered skills.
    pub fn list(&self) -> Vec<Skill> {
        let skills = self.skills.read().unwrap();
        skills.values().cloned().collect()
    }

    /// Get the names of all registered skills.
    pub fn names(&self) -> Vec<String> {
        let skills = self.skills.read().unwrap();
        skills.keys().cloned().collect()
    }

    /// Check if a skill is registered.
    pub fn contains(&self, name: &str) -> bool {
        let skills = self.skills.read().unwrap();
        skills.contains_key(name)
    }

    /// Get the number of registered skills.
    pub fn len(&self) -> usize {
        let skills = self.skills.read().unwrap();
        skills.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all skills from the registry.
    pub fn clear(&self) {
        let mut skills = self.skills.write().unwrap();
        skills.clear();
    }

    /// Generate XML for injection into system prompts.
    ///
    /// The XML format follows the Agent Skills specification:
    /// ```xml
    /// <available_skills>
    ///   <skill>
    ///     <name>skill-name</name>
    ///     <description>Skill description.</description>
    ///     <location>/path/to/SKILL.md</location>
    ///   </skill>
    /// </available_skills>
    /// ```
    pub fn to_prompt_xml(&self) -> String {
        let skills = self.skills.read().unwrap();

        if skills.is_empty() {
            return String::new();
        }

        let mut xml = String::from("<available_skills>\n");

        // Sort by name for consistent output
        let mut sorted_skills: Vec<_> = skills.values().collect();
        sorted_skills.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));

        for skill in sorted_skills {
            xml.push_str("  <skill>\n");
            xml.push_str(&format!(
                "    <name>{}</name>\n",
                escape_xml(&skill.metadata.name)
            ));
            xml.push_str(&format!(
                "    <description>{}</description>\n",
                escape_xml(&skill.metadata.description)
            ));
            xml.push_str(&format!(
                "    <location>{}</location>\n",
                escape_xml(&skill.skill_md_path.display().to_string())
            ));
            xml.push_str("  </skill>\n");
        }

        xml.push_str("</available_skills>");
        xml
    }
}

/// Escape special XML characters.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::types::SkillMetadata;
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

    #[test]
    fn test_register_and_get() {
        let registry = SkillRegistry::new();
        let skill = create_test_skill("test-skill", "A test skill");

        assert!(registry.register(skill).is_none());

        let retrieved = registry.get("test-skill").unwrap();
        assert_eq!(retrieved.metadata.name, "test-skill");
        assert_eq!(retrieved.metadata.description, "A test skill");
    }

    #[test]
    fn test_register_replaces_existing() {
        let registry = SkillRegistry::new();

        let skill1 = create_test_skill("my-skill", "First version");
        let skill2 = create_test_skill("my-skill", "Second version");

        registry.register(skill1);
        let replaced = registry.register(skill2);

        assert!(replaced.is_some());
        assert_eq!(replaced.unwrap().metadata.description, "First version");

        let current = registry.get("my-skill").unwrap();
        assert_eq!(current.metadata.description, "Second version");
    }

    #[test]
    fn test_unregister() {
        let registry = SkillRegistry::new();
        let skill = create_test_skill("to-remove", "Will be removed");

        registry.register(skill);
        assert!(registry.contains("to-remove"));

        let removed = registry.unregister("to-remove");
        assert!(removed.is_some());
        assert!(!registry.contains("to-remove"));

        // Unregister nonexistent returns None
        assert!(registry.unregister("nonexistent").is_none());
    }

    #[test]
    fn test_list() {
        let registry = SkillRegistry::new();

        registry.register(create_test_skill("skill-a", "A"));
        registry.register(create_test_skill("skill-b", "B"));
        registry.register(create_test_skill("skill-c", "C"));

        let skills = registry.list();
        assert_eq!(skills.len(), 3);

        let names: Vec<_> = skills.iter().map(|s| s.metadata.name.as_str()).collect();
        assert!(names.contains(&"skill-a"));
        assert!(names.contains(&"skill-b"));
        assert!(names.contains(&"skill-c"));
    }

    #[test]
    fn test_names() {
        let registry = SkillRegistry::new();

        registry.register(create_test_skill("alpha", "A"));
        registry.register(create_test_skill("beta", "B"));

        let names = registry.names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"alpha".to_string()));
        assert!(names.contains(&"beta".to_string()));
    }

    #[test]
    fn test_len_and_is_empty() {
        let registry = SkillRegistry::new();

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        registry.register(create_test_skill("one", "1"));
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);

        registry.register(create_test_skill("two", "2"));
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_clear() {
        let registry = SkillRegistry::new();

        registry.register(create_test_skill("a", "A"));
        registry.register(create_test_skill("b", "B"));
        assert_eq!(registry.len(), 2);

        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_to_prompt_xml_empty() {
        let registry = SkillRegistry::new();
        let xml = registry.to_prompt_xml();
        assert!(xml.is_empty());
    }

    #[test]
    fn test_to_prompt_xml() {
        let registry = SkillRegistry::new();

        registry.register(create_test_skill("pdf-tools", "Extract text from PDFs"));
        registry.register(create_test_skill("git-helper", "Git operations helper"));

        let xml = registry.to_prompt_xml();

        assert!(xml.starts_with("<available_skills>"));
        assert!(xml.ends_with("</available_skills>"));
        assert!(xml.contains("<name>git-helper</name>"));
        assert!(xml.contains("<name>pdf-tools</name>"));
        assert!(xml.contains("<description>Extract text from PDFs</description>"));
        assert!(xml.contains("<location>/skills/git-helper/SKILL.md</location>"));
    }

    #[test]
    fn test_to_prompt_xml_escapes_special_chars() {
        let registry = SkillRegistry::new();

        registry.register(create_test_skill("test", "Uses <xml> & \"quotes\""));

        let xml = registry.to_prompt_xml();

        assert!(xml.contains("&lt;xml&gt;"));
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("&quot;quotes&quot;"));
    }

    #[test]
    fn test_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let registry = Arc::new(SkillRegistry::new());
        let mut handles = vec![];

        // Spawn multiple threads that register skills
        for i in 0..10 {
            let registry = Arc::clone(&registry);
            handles.push(thread::spawn(move || {
                let skill = create_test_skill(&format!("skill-{}", i), &format!("Skill {}", i));
                registry.register(skill);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(registry.len(), 10);
    }

    /// Integration test: Discovery -> Registry -> XML workflow
    #[test]
    fn test_discovery_to_xml_workflow() {
        use crate::skills::SkillDiscovery;
        use std::fs;
        use tempfile::TempDir;

        // Create test skills directory
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: my-skill
description: A test skill for integration testing.
---

# My Skill

Instructions for the LLM.
"#,
        )
        .unwrap();

        // Discover skills
        let mut discovery = SkillDiscovery::empty();
        discovery.add_path(temp_dir.path().to_path_buf());
        let discovered = discovery.discover_valid();
        assert_eq!(discovered.len(), 1);

        // Register in registry
        let registry = SkillRegistry::new();
        for skill in discovered {
            registry.register(skill);
        }

        // Generate XML for system prompt
        let xml = registry.to_prompt_xml();

        // Verify XML structure
        assert!(xml.contains("<available_skills>"));
        assert!(xml.contains("</available_skills>"));
        assert!(xml.contains("<name>my-skill</name>"));
        assert!(xml.contains("<description>A test skill for integration testing.</description>"));
        assert!(xml.contains("SKILL.md</location>"));
    }
}
