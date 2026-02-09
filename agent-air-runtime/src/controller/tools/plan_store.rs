//! Shared state and utilities for plan tools.
//!
//! Provides the `PlanStore` struct that manages the plans directory,
//! per-file locking, sequential plan ID generation, and status marker
//! conversion helpers used by both `MarkdownPlanTool` and `UpdatePlanStepTool`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

/// Directory name under workspace root where plans are stored.
const PLANS_DIR_NAME: &str = ".agent-air/plans";

/// Status marker for pending steps.
pub const PENDING_MARKER: &str = " ";
/// Status marker for in-progress steps.
pub const IN_PROGRESS_MARKER: &str = "~";
/// Status marker for completed steps.
pub const COMPLETED_MARKER: &str = "x";
/// Status marker for skipped steps.
pub const SKIPPED_MARKER: &str = "-";

/// Shared state for plan tools, managing the plans directory and per-file locks.
pub struct PlanStore {
    /// Resolved path to the plans directory.
    plans_dir: PathBuf,
    /// Per-file mutexes to prevent concurrent writes to the same plan file.
    file_locks: RwLock<HashMap<PathBuf, Arc<Mutex<()>>>>,
}

impl PlanStore {
    /// Create a new `PlanStore` rooted at the given workspace directory.
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            plans_dir: workspace_root.join(PLANS_DIR_NAME),
            file_locks: RwLock::new(HashMap::new()),
        }
    }

    /// Returns the resolved plans directory path.
    pub fn plans_dir(&self) -> &Path {
        &self.plans_dir
    }

    /// Returns a per-file mutex for the given path, creating one if it does not exist.
    pub async fn acquire_lock(&self, path: &Path) -> Arc<Mutex<()>> {
        // Fast path: check if lock already exists.
        {
            let locks = self.file_locks.read().await;
            if let Some(lock) = locks.get(path) {
                return lock.clone();
            }
        }

        // Slow path: create a new lock.
        let mut locks = self.file_locks.write().await;
        locks
            .entry(path.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Scans the plans directory for existing `plan-NNN.md` files and returns
    /// the next sequential plan ID (e.g., `plan-001`, `plan-002`).
    pub async fn get_next_plan_id(&self) -> Result<String, String> {
        let plans_dir = &self.plans_dir;

        // If the directory doesn't exist yet, start at 001.
        if !plans_dir.exists() {
            return Ok("plan-001".to_string());
        }

        let mut max_num: u32 = 0;

        let mut entries = tokio::fs::read_dir(plans_dir)
            .await
            .map_err(|e| format!("Failed to read plans directory: {}", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("Failed to read directory entry: {}", e))?
        {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if let Some(num_str) = name
                .strip_prefix("plan-")
                .and_then(|s| s.strip_suffix(".md"))
                && let Ok(num) = num_str.parse::<u32>()
                && num > max_num
            {
                max_num = num;
            }
        }

        Ok(format!("plan-{:03}", max_num + 1))
    }

    /// Converts a step status string to its markdown checkbox marker.
    pub fn status_to_marker(status: &str) -> Result<&'static str, String> {
        match status {
            "pending" => Ok(PENDING_MARKER),
            "in_progress" => Ok(IN_PROGRESS_MARKER),
            "completed" => Ok(COMPLETED_MARKER),
            "skipped" => Ok(SKIPPED_MARKER),
            _ => Err(format!(
                "Invalid step status '{}'. Must be one of: pending, in_progress, completed, skipped",
                status
            )),
        }
    }

    /// Converts a markdown checkbox marker to its status string.
    pub fn marker_to_status(marker: &str) -> &'static str {
        match marker {
            " " => "pending",
            "~" => "in_progress",
            "x" => "completed",
            "-" => "skipped",
            _ => "pending",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_get_next_plan_id_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let store = PlanStore::new(temp_dir.path().to_path_buf());

        // Plans directory doesn't exist yet — should return plan-001.
        let id = store.get_next_plan_id().await.unwrap();
        assert_eq!(id, "plan-001");
    }

    #[tokio::test]
    async fn test_get_next_plan_id_existing_plans() {
        let temp_dir = TempDir::new().unwrap();
        let store = PlanStore::new(temp_dir.path().to_path_buf());

        // Create the plans directory and some plan files.
        let plans_dir = temp_dir.path().join(PLANS_DIR_NAME);
        tokio::fs::create_dir_all(&plans_dir).await.unwrap();
        tokio::fs::write(plans_dir.join("plan-001.md"), "# Plan 1")
            .await
            .unwrap();
        tokio::fs::write(plans_dir.join("plan-003.md"), "# Plan 3")
            .await
            .unwrap();
        // Non-matching files should be ignored.
        tokio::fs::write(plans_dir.join("notes.md"), "# Notes")
            .await
            .unwrap();

        let id = store.get_next_plan_id().await.unwrap();
        assert_eq!(id, "plan-004");
    }

    #[test]
    fn test_plans_dir_derived_from_workspace_root() {
        let store = PlanStore::new(PathBuf::from("/workspace/root"));
        assert_eq!(
            store.plans_dir(),
            Path::new("/workspace/root/.agent-air/plans")
        );
    }

    #[test]
    fn test_status_to_marker() {
        assert_eq!(PlanStore::status_to_marker("pending").unwrap(), " ");
        assert_eq!(PlanStore::status_to_marker("in_progress").unwrap(), "~");
        assert_eq!(PlanStore::status_to_marker("completed").unwrap(), "x");
        assert_eq!(PlanStore::status_to_marker("skipped").unwrap(), "-");
        assert!(PlanStore::status_to_marker("invalid").is_err());
    }

    #[test]
    fn test_marker_to_status() {
        assert_eq!(PlanStore::marker_to_status(" "), "pending");
        assert_eq!(PlanStore::marker_to_status("~"), "in_progress");
        assert_eq!(PlanStore::marker_to_status("x"), "completed");
        assert_eq!(PlanStore::marker_to_status("-"), "skipped");
        // Unknown markers default to pending.
        assert_eq!(PlanStore::marker_to_status("?"), "pending");
    }

    #[tokio::test]
    async fn test_acquire_lock_returns_same_lock_for_same_path() {
        let temp_dir = TempDir::new().unwrap();
        let store = PlanStore::new(temp_dir.path().to_path_buf());

        let path = PathBuf::from("/some/plan.md");
        let lock1 = store.acquire_lock(&path).await;
        let lock2 = store.acquire_lock(&path).await;

        // Both should point to the same underlying mutex.
        assert!(Arc::ptr_eq(&lock1, &lock2));
    }

    #[tokio::test]
    async fn test_acquire_lock_returns_different_locks_for_different_paths() {
        let temp_dir = TempDir::new().unwrap();
        let store = PlanStore::new(temp_dir.path().to_path_buf());

        let path_a = PathBuf::from("/some/plan-a.md");
        let path_b = PathBuf::from("/some/plan-b.md");
        let lock_a = store.acquire_lock(&path_a).await;
        let lock_b = store.acquire_lock(&path_b).await;

        assert!(!Arc::ptr_eq(&lock_a, &lock_b));
    }
}
