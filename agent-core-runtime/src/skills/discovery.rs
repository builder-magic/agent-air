//! Skill discovery by scanning directories for SKILL.md files.

use crate::skills::parser::parse_skill_md;
use crate::skills::types::{Skill, SkillDiscoveryError};
use std::path::PathBuf;

/// Default skill directory name within a project.
const PROJECT_SKILLS_DIR: &str = ".skills";

/// Default user skills directory name.
const USER_SKILLS_DIR: &str = ".agent-core/skills";

/// Discovers skills from configured directories.
#[derive(Debug, Default)]
pub struct SkillDiscovery {
    search_paths: Vec<PathBuf>,
}

impl SkillDiscovery {
    /// Create a new skill discovery instance with default search paths.
    ///
    /// Default paths:
    /// 1. `$PWD/.skills/` - Project-local skills
    /// 2. `~/.agent-core/skills/` - User skills
    pub fn new() -> Self {
        let mut discovery = Self::default();

        // Add project-local skills directory
        if let Ok(cwd) = std::env::current_dir() {
            let project_skills = cwd.join(PROJECT_SKILLS_DIR);
            discovery.add_path(project_skills);
        }

        // Add user skills directory
        if let Some(home) = dirs::home_dir() {
            let user_skills = home.join(USER_SKILLS_DIR);
            discovery.add_path(user_skills);
        }

        discovery
    }

    /// Create an empty skill discovery instance with no default paths.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Add a search path for skill discovery.
    pub fn add_path(&mut self, path: PathBuf) {
        if !self.search_paths.contains(&path) {
            self.search_paths.push(path);
        }
    }

    /// Get the configured search paths.
    pub fn search_paths(&self) -> &[PathBuf] {
        &self.search_paths
    }

    /// Discover all skills from configured directories.
    ///
    /// Returns a vector of results, one for each skill found or error encountered.
    /// Each immediate subdirectory containing a SKILL.md file is treated as a skill.
    pub fn discover(&self) -> Vec<Result<Skill, SkillDiscoveryError>> {
        let mut results = Vec::new();

        for search_path in &self.search_paths {
            if !search_path.exists() {
                continue;
            }

            if !search_path.is_dir() {
                results.push(Err(SkillDiscoveryError::new(
                    search_path.clone(),
                    "Search path is not a directory",
                )));
                continue;
            }

            // Look for immediate subdirectories containing SKILL.md
            let entries = match std::fs::read_dir(search_path) {
                Ok(entries) => entries,
                Err(e) => {
                    results.push(Err(SkillDiscoveryError::new(
                        search_path.clone(),
                        format!("Failed to read directory: {}", e),
                    )));
                    continue;
                }
            };

            for entry in entries.flatten() {
                let skill_dir = entry.path();
                if !skill_dir.is_dir() {
                    continue;
                }

                let skill_md_path = skill_dir.join("SKILL.md");
                if !skill_md_path.exists() {
                    continue;
                }

                match parse_skill_md(&skill_md_path) {
                    Ok(metadata) => {
                        let skill = Skill {
                            metadata,
                            path: skill_dir.canonicalize().unwrap_or(skill_dir),
                            skill_md_path: skill_md_path.canonicalize().unwrap_or(skill_md_path),
                        };
                        results.push(Ok(skill));
                    }
                    Err(e) => {
                        results.push(Err(e));
                    }
                }
            }
        }

        results
    }

    /// Discover skills and collect only the successful ones.
    ///
    /// Errors are silently ignored. Use `discover()` if you need error information.
    pub fn discover_valid(&self) -> Vec<Skill> {
        self.discover().into_iter().filter_map(Result::ok).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_skill(dir: &TempDir, name: &str, description: &str) -> PathBuf {
        let skill_dir = dir.path().join(name);
        fs::create_dir_all(&skill_dir).unwrap();

        let skill_md = skill_dir.join("SKILL.md");
        let content = format!(
            r#"---
name: {}
description: {}
---

# {}

Instructions here.
"#,
            name, description, name
        );
        fs::write(&skill_md, content).unwrap();

        skill_dir
    }

    #[test]
    fn test_discover_skills() {
        let temp_dir = TempDir::new().unwrap();

        create_skill(&temp_dir, "skill-one", "First test skill");
        create_skill(&temp_dir, "skill-two", "Second test skill");

        let mut discovery = SkillDiscovery::empty();
        discovery.add_path(temp_dir.path().to_path_buf());

        let results = discovery.discover();
        let skills: Vec<_> = results.into_iter().filter_map(Result::ok).collect();

        assert_eq!(skills.len(), 2);

        let names: Vec<_> = skills.iter().map(|s| s.metadata.name.as_str()).collect();
        assert!(names.contains(&"skill-one"));
        assert!(names.contains(&"skill-two"));
    }

    #[test]
    fn test_discover_ignores_non_skill_dirs() {
        let temp_dir = TempDir::new().unwrap();

        // Create a valid skill
        create_skill(&temp_dir, "valid-skill", "A valid skill");

        // Create a directory without SKILL.md
        let other_dir = temp_dir.path().join("other-dir");
        fs::create_dir_all(&other_dir).unwrap();
        fs::write(other_dir.join("README.md"), "Not a skill").unwrap();

        // Create a file (not directory)
        fs::write(temp_dir.path().join("random-file.txt"), "Not a skill").unwrap();

        let mut discovery = SkillDiscovery::empty();
        discovery.add_path(temp_dir.path().to_path_buf());

        let skills = discovery.discover_valid();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].metadata.name, "valid-skill");
    }

    #[test]
    fn test_discover_reports_invalid_skills() {
        let temp_dir = TempDir::new().unwrap();

        // Create an invalid skill (bad name)
        let skill_dir = temp_dir.path().join("invalid");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: InvalidName
description: Bad name format.
---
"#,
        )
        .unwrap();

        let mut discovery = SkillDiscovery::empty();
        discovery.add_path(temp_dir.path().to_path_buf());

        let results = discovery.discover();

        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
    }

    #[test]
    fn test_discover_nonexistent_path() {
        let mut discovery = SkillDiscovery::empty();
        discovery.add_path(PathBuf::from("/nonexistent/path/that/does/not/exist"));

        let results = discovery.discover();

        // Should return empty, not error
        assert!(results.is_empty());
    }

    #[test]
    fn test_add_path_deduplication() {
        let mut discovery = SkillDiscovery::empty();
        let path = PathBuf::from("/some/path");

        discovery.add_path(path.clone());
        discovery.add_path(path.clone());
        discovery.add_path(path);

        assert_eq!(discovery.search_paths().len(), 1);
    }

    #[test]
    fn test_discover_from_multiple_paths() {
        // Test that skills can be loaded from multiple custom paths
        // This validates the pattern used by load_skills_from()
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        create_skill(&temp_dir1, "skill-from-path1", "Skill from first path");
        create_skill(&temp_dir2, "skill-from-path2", "Skill from second path");

        let mut discovery = SkillDiscovery::empty();
        discovery.add_path(temp_dir1.path().to_path_buf());
        discovery.add_path(temp_dir2.path().to_path_buf());

        let skills = discovery.discover_valid();
        assert_eq!(skills.len(), 2);

        let names: Vec<_> = skills.iter().map(|s| s.metadata.name.as_str()).collect();
        assert!(names.contains(&"skill-from-path1"));
        assert!(names.contains(&"skill-from-path2"));
    }

    #[test]
    fn test_duplicate_skill_names_across_paths() {
        // Test behavior when two paths have skills with the same name
        // The second discovered skill should be returned (order depends on path order)
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        create_skill(&temp_dir1, "same-name", "First version from path1");
        create_skill(&temp_dir2, "same-name", "Second version from path2");

        let mut discovery = SkillDiscovery::empty();
        discovery.add_path(temp_dir1.path().to_path_buf());
        discovery.add_path(temp_dir2.path().to_path_buf());

        let results = discovery.discover();

        // Both skills are discovered (registry handles deduplication, not discovery)
        let valid: Vec<_> = results.into_iter().filter_map(Result::ok).collect();
        assert_eq!(valid.len(), 2);

        // Both have the same name
        assert!(valid.iter().all(|s| s.metadata.name == "same-name"));
    }
}
