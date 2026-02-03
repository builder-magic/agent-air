//! Parser for SKILL.md files with YAML frontmatter.

use crate::skills::types::{SkillDiscoveryError, SkillMetadata};
use std::path::Path;

/// Maximum allowed length for skill name.
const MAX_NAME_LENGTH: usize = 64;

/// Maximum allowed length for skill description.
const MAX_DESCRIPTION_LENGTH: usize = 1024;

/// Parse a SKILL.md file and extract the metadata from its YAML frontmatter.
///
/// The file must start with `---` followed by YAML content and another `---`.
pub fn parse_skill_md(path: &Path) -> Result<SkillMetadata, SkillDiscoveryError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| SkillDiscoveryError::new(path.to_path_buf(), format!("Failed to read file: {}", e)))?;

    let frontmatter = extract_frontmatter(&content)
        .ok_or_else(|| SkillDiscoveryError::new(path.to_path_buf(), "Missing or invalid YAML frontmatter"))?;

    let metadata: SkillMetadata = serde_yaml::from_str(frontmatter)
        .map_err(|e| SkillDiscoveryError::new(path.to_path_buf(), format!("Invalid YAML: {}", e)))?;

    validate_metadata(&metadata, path)?;

    Ok(metadata)
}

/// Extract YAML frontmatter from content.
///
/// Frontmatter must be delimited by `---` at the start and end.
fn extract_frontmatter(content: &str) -> Option<&str> {
    let content = content.trim_start();

    if !content.starts_with("---") {
        return None;
    }

    let after_first_delim = &content[3..];
    let end_pos = after_first_delim.find("\n---")?;

    Some(&after_first_delim[..end_pos])
}

/// Validate skill metadata according to the spec.
fn validate_metadata(metadata: &SkillMetadata, path: &Path) -> Result<(), SkillDiscoveryError> {
    validate_name(&metadata.name, path)?;
    validate_description(&metadata.description, path)?;
    Ok(())
}

/// Validate skill name format.
///
/// Name must be 1-64 characters, lowercase letters, numbers, and hyphens only.
/// Cannot start or end with a hyphen.
fn validate_name(name: &str, path: &Path) -> Result<(), SkillDiscoveryError> {
    if name.is_empty() {
        return Err(SkillDiscoveryError::new(path.to_path_buf(), "Skill name cannot be empty"));
    }

    if name.len() > MAX_NAME_LENGTH {
        return Err(SkillDiscoveryError::new(
            path.to_path_buf(),
            format!("Skill name exceeds {} characters", MAX_NAME_LENGTH),
        ));
    }

    if name.starts_with('-') || name.ends_with('-') {
        return Err(SkillDiscoveryError::new(
            path.to_path_buf(),
            "Skill name cannot start or end with a hyphen",
        ));
    }

    for c in name.chars() {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
            return Err(SkillDiscoveryError::new(
                path.to_path_buf(),
                format!("Skill name contains invalid character '{}'. Only lowercase letters, numbers, and hyphens allowed", c),
            ));
        }
    }

    Ok(())
}

/// Validate skill description.
fn validate_description(description: &str, path: &Path) -> Result<(), SkillDiscoveryError> {
    if description.is_empty() {
        return Err(SkillDiscoveryError::new(path.to_path_buf(), "Skill description cannot be empty"));
    }

    if description.len() > MAX_DESCRIPTION_LENGTH {
        return Err(SkillDiscoveryError::new(
            path.to_path_buf(),
            format!("Skill description exceeds {} characters", MAX_DESCRIPTION_LENGTH),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_skill_md(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_parse_valid_skill_md() {
        let content = r#"---
name: test-skill
description: A test skill for unit testing.
license: MIT
---

# Test Skill

Instructions here.
"#;
        let file = create_temp_skill_md(content);
        let metadata = parse_skill_md(file.path()).unwrap();

        assert_eq!(metadata.name, "test-skill");
        assert_eq!(metadata.description, "A test skill for unit testing.");
        assert_eq!(metadata.license, Some("MIT".to_string()));
    }

    #[test]
    fn test_parse_minimal_skill_md() {
        let content = r#"---
name: minimal
description: Minimal skill.
---
"#;
        let file = create_temp_skill_md(content);
        let metadata = parse_skill_md(file.path()).unwrap();

        assert_eq!(metadata.name, "minimal");
        assert_eq!(metadata.description, "Minimal skill.");
        assert!(metadata.license.is_none());
        assert!(metadata.compatibility.is_none());
    }

    #[test]
    fn test_parse_with_metadata() {
        let content = r#"---
name: with-metadata
description: Skill with extra metadata.
metadata:
  author: test-org
  version: "1.0"
---
"#;
        let file = create_temp_skill_md(content);
        let metadata = parse_skill_md(file.path()).unwrap();

        let extra = metadata.metadata.unwrap();
        assert_eq!(extra.get("author"), Some(&"test-org".to_string()));
        assert_eq!(extra.get("version"), Some(&"1.0".to_string()));
    }

    #[test]
    fn test_missing_frontmatter() {
        let content = "# No frontmatter here";
        let file = create_temp_skill_md(content);
        let result = parse_skill_md(file.path());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("frontmatter"));
    }

    #[test]
    fn test_invalid_name_uppercase() {
        let content = r#"---
name: TestSkill
description: Invalid name.
---
"#;
        let file = create_temp_skill_md(content);
        let result = parse_skill_md(file.path());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid character"));
    }

    #[test]
    fn test_invalid_name_starts_with_hyphen() {
        let content = r#"---
name: -invalid
description: Invalid name.
---
"#;
        let file = create_temp_skill_md(content);
        let result = parse_skill_md(file.path());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("hyphen"));
    }

    #[test]
    fn test_empty_description() {
        let content = r#"---
name: valid-name
description: ""
---
"#;
        let file = create_temp_skill_md(content);
        let result = parse_skill_md(file.path());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("description"));
    }

    #[test]
    fn test_extract_frontmatter() {
        let content = "---\nname: test\n---\nBody";
        let fm = extract_frontmatter(content).unwrap();
        assert_eq!(fm.trim(), "name: test");
    }

    #[test]
    fn test_extract_frontmatter_with_leading_whitespace() {
        let content = "  \n---\nname: test\n---\nBody";
        let fm = extract_frontmatter(content).unwrap();
        assert_eq!(fm.trim(), "name: test");
    }
}
