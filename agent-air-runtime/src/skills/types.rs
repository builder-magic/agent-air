//! Core types for the Agent Skills system.
//!
//! Skills are folders containing a SKILL.md file with YAML frontmatter and
//! Markdown instructions. See <https://agentskills.io> for the specification.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Metadata parsed from the YAML frontmatter of a SKILL.md file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Skill name (required). Must be 1-64 chars, lowercase with hyphens.
    pub name: String,

    /// Skill description (required). Must be 1-1024 chars.
    pub description: String,

    /// License identifier (optional).
    #[serde(default)]
    pub license: Option<String>,

    /// Compatibility notes (optional).
    #[serde(default)]
    pub compatibility: Option<String>,

    /// Additional metadata key-value pairs (optional).
    #[serde(default)]
    pub metadata: Option<HashMap<String, String>>,

    /// Allowed tools specification (optional, experimental).
    #[serde(default, rename = "allowed-tools")]
    pub allowed_tools: Option<String>,
}

/// A discovered skill with its metadata and file paths.
#[derive(Debug, Clone)]
pub struct Skill {
    /// Parsed metadata from SKILL.md frontmatter.
    pub metadata: SkillMetadata,

    /// Absolute path to the skill directory.
    pub path: PathBuf,

    /// Absolute path to the SKILL.md file.
    pub skill_md_path: PathBuf,
}

/// Result of a skill reload operation.
#[derive(Debug, Default)]
pub struct SkillReloadResult {
    /// Names of newly added skills.
    pub added: Vec<String>,

    /// Names of removed skills.
    pub removed: Vec<String>,

    /// Errors encountered during discovery.
    pub errors: Vec<SkillDiscoveryError>,
}

/// Errors that can occur during skill discovery.
#[derive(Debug, Clone)]
pub struct SkillDiscoveryError {
    /// Path where the error occurred.
    pub path: PathBuf,

    /// Description of what went wrong.
    pub message: String,
}

impl std::fmt::Display for SkillDiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for SkillDiscoveryError {}

impl SkillDiscoveryError {
    pub fn new(path: PathBuf, message: impl Into<String>) -> Self {
        Self {
            path,
            message: message.into(),
        }
    }
}
