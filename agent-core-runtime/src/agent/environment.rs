//! Environment context for system prompts.
//!
//! Gathers working directory and system information to include in
//! the LLM system prompt, giving the model awareness of its environment.

use std::env;
use std::path::PathBuf;

use chrono::Local;

/// Environment context information.
///
/// Contains details about the current working environment that can be
/// included in the system prompt to give the LLM context awareness.
#[derive(Debug, Clone)]
pub struct EnvironmentContext {
    /// Current working directory.
    pub working_directory: PathBuf,
    /// Operating system (e.g., "darwin", "linux", "windows").
    pub platform: String,
    /// OS version string (e.g., "Darwin 25.2.0").
    pub os_version: Option<String>,
    /// Today's date in YYYY-MM-DD format.
    pub date: String,
}

impl EnvironmentContext {
    /// Gather environment context from the current system.
    ///
    /// This collects:
    /// - Current working directory
    /// - Platform (OS type)
    /// - OS version (via uname on Unix, ver on Windows)
    /// - Current date
    pub fn gather() -> Self {
        let working_directory = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let platform = env::consts::OS.to_string();
        let os_version = Self::get_os_version();
        let date = Local::now().format("%Y-%m-%d").to_string();

        Self {
            working_directory,
            platform,
            os_version,
            date,
        }
    }

    /// Get the OS version string.
    #[cfg(unix)]
    fn get_os_version() -> Option<String> {
        use std::process::Command;

        let output = Command::new("uname").arg("-rs").output().ok()?;

        if output.status.success() {
            String::from_utf8(output.stdout)
                .ok()
                .map(|s| s.trim().to_string())
        } else {
            None
        }
    }

    /// Get the OS version string on Windows.
    #[cfg(windows)]
    fn get_os_version() -> Option<String> {
        use std::process::Command;

        let output = Command::new("cmd").args(["/C", "ver"]).output().ok()?;

        if output.status.success() {
            String::from_utf8(output.stdout)
                .ok()
                .map(|s| s.trim().to_string())
        } else {
            None
        }
    }

    /// Fallback for other platforms.
    #[cfg(not(any(unix, windows)))]
    fn get_os_version() -> Option<String> {
        None
    }

    /// Format the environment context as a system prompt section.
    ///
    /// Returns a string wrapped in `<env>` tags suitable for appending
    /// to a system prompt.
    pub fn to_prompt_section(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!(
            "Working directory: {}",
            self.working_directory.display()
        ));
        lines.push(format!("Platform: {}", self.platform));

        if let Some(ref version) = self.os_version {
            lines.push(format!("OS Version: {}", version));
        }

        lines.push(format!("Today's date: {}", self.date));

        format!("<env>\n{}\n</env>", lines.join("\n"))
    }
}

impl Default for EnvironmentContext {
    fn default() -> Self {
        Self::gather()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gather_environment() {
        let ctx = EnvironmentContext::gather();

        // Should have a working directory
        assert!(!ctx.working_directory.as_os_str().is_empty());

        // Should have a platform
        assert!(!ctx.platform.is_empty());

        // Date should be in YYYY-MM-DD format
        assert_eq!(ctx.date.len(), 10);
        assert!(ctx.date.contains('-'));
    }

    #[test]
    fn test_to_prompt_section() {
        let ctx = EnvironmentContext {
            working_directory: PathBuf::from("/test/path"),
            platform: "darwin".to_string(),
            os_version: Some("Darwin 25.2.0".to_string()),
            date: "2026-01-31".to_string(),
        };

        let section = ctx.to_prompt_section();

        assert!(section.starts_with("<env>"));
        assert!(section.ends_with("</env>"));
        assert!(section.contains("Working directory: /test/path"));
        assert!(section.contains("Platform: darwin"));
        assert!(section.contains("OS Version: Darwin 25.2.0"));
        assert!(section.contains("Today's date: 2026-01-31"));
    }

    #[test]
    fn test_to_prompt_section_without_os_version() {
        let ctx = EnvironmentContext {
            working_directory: PathBuf::from("/test/path"),
            platform: "unknown".to_string(),
            os_version: None,
            date: "2026-01-31".to_string(),
        };

        let section = ctx.to_prompt_section();

        assert!(!section.contains("OS Version:"));
        assert!(section.contains("Platform: unknown"));
    }
}
