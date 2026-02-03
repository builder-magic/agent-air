//! Agent Skills support for extended agent capabilities.
//!
//! This module implements the [Agent Skills](https://agentskills.io) open format,
//! allowing agents to discover and use skills for extended capabilities.
//!
//! # Overview
//!
//! Skills are folders containing a `SKILL.md` file with YAML frontmatter and
//! Markdown instructions:
//!
//! ```text
//! my-skill/
//! ├── SKILL.md          # Required: metadata + instructions
//! ├── scripts/          # Optional: executable code
//! ├── references/       # Optional: documentation
//! └── assets/           # Optional: templates, resources
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use agent_core_runtime::skills::{SkillDiscovery, SkillRegistry};
//!
//! // Create discovery with default paths
//! let discovery = SkillDiscovery::new();
//!
//! // Discover skills
//! let registry = SkillRegistry::new();
//! for result in discovery.discover() {
//!     match result {
//!         Ok(skill) => {
//!             println!("Found skill: {}", skill.metadata.name);
//!             registry.register(skill);
//!         }
//!         Err(e) => eprintln!("Error: {}", e),
//!     }
//! }
//!
//! // Generate XML for system prompt
//! let xml = registry.to_prompt_xml();
//! ```

mod discovery;
mod parser;
mod registry;
mod types;

pub use discovery::SkillDiscovery;
pub use parser::parse_skill_md;
pub use registry::SkillRegistry;
pub use types::{Skill, SkillDiscoveryError, SkillMetadata, SkillReloadResult};
