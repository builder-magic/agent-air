//! Batch permission request handling.
//!
//! When multiple tools run in parallel, their permission requests are
//! collected into a batch and presented to the user together, avoiding
//! the deadlock issue with sequential permission prompts.

use super::{Grant, GrantTarget, PermissionLevel, PermissionRequest};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// A batch of permission requests from parallel tool executions.
///
/// Batching permission requests allows the UI to present multiple
/// requests together, letting the user make informed decisions about
/// granting access to related resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPermissionRequest {
    /// Unique identifier for this batch.
    pub batch_id: String,
    /// The individual permission requests in this batch.
    pub requests: Vec<PermissionRequest>,
    /// Suggested grants that would cover all requests.
    pub suggested_grants: Vec<Grant>,
}

impl BatchPermissionRequest {
    /// Creates a new batch permission request.
    pub fn new(batch_id: impl Into<String>, requests: Vec<PermissionRequest>) -> Self {
        let batch_id = batch_id.into();
        let suggested_grants = compute_suggested_grants(&requests);
        Self {
            batch_id,
            requests,
            suggested_grants,
        }
    }

    /// Returns the number of requests in this batch.
    pub fn len(&self) -> usize {
        self.requests.len()
    }

    /// Returns true if the batch has no requests.
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    /// Returns the unique request IDs in this batch.
    pub fn request_ids(&self) -> Vec<&str> {
        self.requests.iter().map(|r| r.id.as_str()).collect()
    }
}

/// Response to a batch permission request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPermissionResponse {
    /// The batch ID this response is for.
    pub batch_id: String,
    /// Grants that were approved by the user.
    pub approved_grants: Vec<Grant>,
    /// Request IDs that were explicitly denied.
    pub denied_requests: HashSet<String>,
    /// Request IDs that were auto-approved (already had permission).
    pub auto_approved: HashSet<String>,
}

impl BatchPermissionResponse {
    /// Creates a response where all requests were granted.
    pub fn all_granted(batch_id: impl Into<String>, grants: Vec<Grant>) -> Self {
        Self {
            batch_id: batch_id.into(),
            approved_grants: grants,
            denied_requests: HashSet::new(),
            auto_approved: HashSet::new(),
        }
    }

    /// Creates a response where all requests were denied.
    pub fn all_denied(
        batch_id: impl Into<String>,
        request_ids: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            batch_id: batch_id.into(),
            approved_grants: Vec::new(),
            denied_requests: request_ids.into_iter().collect(),
            auto_approved: HashSet::new(),
        }
    }

    /// Creates a response with auto-approved requests.
    pub fn with_auto_approved(
        batch_id: impl Into<String>,
        auto_approved: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            batch_id: batch_id.into(),
            approved_grants: Vec::new(),
            denied_requests: HashSet::new(),
            auto_approved: auto_approved.into_iter().collect(),
        }
    }

    /// Checks if a specific request was granted (either explicitly or auto-approved).
    ///
    /// # Conflict Resolution
    /// If a request_id appears in both `auto_approved` and `denied_requests`,
    /// this is treated as a malformed response. A warning is logged and the
    /// request is denied (safe default).
    pub fn is_granted(&self, request_id: &str, request: &PermissionRequest) -> bool {
        // Validate: request cannot be both auto-approved and denied
        let in_auto_approved = self.auto_approved.contains(request_id);
        let in_denied = self.denied_requests.contains(request_id);

        if in_auto_approved && in_denied {
            tracing::warn!(
                request_id,
                batch_id = %self.batch_id,
                "Request appears in both auto_approved and denied_requests, treating as denied"
            );
            return false;
        }

        // Check if auto-approved
        if in_auto_approved {
            return true;
        }

        // Check if denied
        if in_denied {
            return false;
        }

        // Check if any approved grant satisfies this request
        self.approved_grants
            .iter()
            .any(|grant| grant.satisfies(request))
    }

    /// Returns whether any requests were denied.
    pub fn has_denials(&self) -> bool {
        !self.denied_requests.is_empty()
    }

    /// Returns the number of approved grants.
    pub fn approved_count(&self) -> usize {
        self.approved_grants.len() + self.auto_approved.len()
    }
}

/// Computes suggested grants that would cover all requests.
///
/// This function analyzes the requests and suggests a minimal set of
/// grants that would satisfy all of them. It groups requests by common
/// parent directories and suggests recursive grants where appropriate.
pub fn compute_suggested_grants(requests: &[PermissionRequest]) -> Vec<Grant> {
    if requests.is_empty() {
        return Vec::new();
    }

    let mut grants = Vec::new();

    // Group requests by target type
    let mut path_requests: Vec<&PermissionRequest> = Vec::new();
    let mut domain_requests: Vec<&PermissionRequest> = Vec::new();
    let mut command_requests: Vec<&PermissionRequest> = Vec::new();

    for req in requests {
        match &req.target {
            GrantTarget::Path { .. } => path_requests.push(req),
            GrantTarget::Domain { .. } => domain_requests.push(req),
            GrantTarget::Command { .. } => command_requests.push(req),
        }
    }

    // Compute grants for each target type
    grants.extend(compute_path_grants(&path_requests));
    grants.extend(compute_domain_grants(&domain_requests));
    grants.extend(compute_command_grants(&command_requests));

    grants
}

/// Computes suggested grants for path-based requests.
fn compute_path_grants(requests: &[&PermissionRequest]) -> Vec<Grant> {
    if requests.is_empty() {
        return Vec::new();
    }

    // Group paths by their parent directories and track max level needed
    let mut dir_groups: HashMap<PathBuf, (PermissionLevel, Vec<PathBuf>)> = HashMap::new();

    for req in requests {
        if let GrantTarget::Path { path, .. } = &req.target {
            let parent = path.parent().unwrap_or(path).to_path_buf();
            let entry = dir_groups
                .entry(parent)
                .or_insert((PermissionLevel::None, Vec::new()));
            entry.0 = std::cmp::max(entry.0, req.required_level);
            entry.1.push(path.clone());
        }
    }

    // Try to find common ancestors for related directories
    let merged_groups = merge_related_directories(dir_groups);

    // Create grants for each group
    merged_groups
        .into_iter()
        .map(|(dir, (level, paths))| {
            // If multiple paths share the same parent, make it recursive
            let recursive = paths.len() > 1;
            Grant::new(GrantTarget::path(dir, recursive), level)
        })
        .collect()
}

/// Merges directory groups that share a common ancestor.
fn merge_related_directories(
    groups: HashMap<PathBuf, (PermissionLevel, Vec<PathBuf>)>,
) -> HashMap<PathBuf, (PermissionLevel, Vec<PathBuf>)> {
    if groups.len() <= 1 {
        return groups;
    }

    let mut result: HashMap<PathBuf, (PermissionLevel, Vec<PathBuf>)> = HashMap::new();

    for (dir, (level, paths)) in groups {
        // Check if this directory can be merged with an existing one
        let mut merged = false;

        for (existing_dir, (existing_level, existing_paths)) in result.iter_mut() {
            // Check if one is ancestor of the other
            if dir.starts_with(existing_dir) {
                // Existing dir is ancestor - add paths and update level
                existing_paths.extend(paths.clone());
                *existing_level = std::cmp::max(*existing_level, level);
                merged = true;
                break;
            } else if existing_dir.starts_with(&dir) {
                // New dir is ancestor - this case needs special handling
                // For simplicity, we'll just add as separate entry
            } else {
                // Check for common ancestor within reasonable depth
                if let Some(common) = find_common_ancestor(&dir, existing_dir, 3) {
                    // If close enough, could merge under common ancestor
                    // For now, keep separate to avoid over-granting
                    let _ = common;
                }
            }
        }

        if !merged {
            result.insert(dir, (level, paths));
        }
    }

    result
}

/// Finds the common ancestor of two paths, up to a maximum depth from either path.
fn find_common_ancestor(path1: &PathBuf, path2: &PathBuf, max_depth: usize) -> Option<PathBuf> {
    let ancestors1: Vec<_> = path1.ancestors().take(max_depth + 1).collect();
    let ancestors2: Vec<_> = path2.ancestors().take(max_depth + 1).collect();

    for a1 in &ancestors1 {
        for a2 in &ancestors2 {
            if a1 == a2 {
                return Some(a1.to_path_buf());
            }
        }
    }

    None
}

/// Computes suggested grants for domain-based requests.
fn compute_domain_grants(requests: &[&PermissionRequest]) -> Vec<Grant> {
    if requests.is_empty() {
        return Vec::new();
    }

    // Group by base domain and track max level
    let mut domain_levels: HashMap<String, PermissionLevel> = HashMap::new();

    for req in requests {
        if let GrantTarget::Domain { pattern } = &req.target {
            let base_domain = extract_base_domain(pattern);
            let entry = domain_levels
                .entry(base_domain)
                .or_insert(PermissionLevel::None);
            *entry = std::cmp::max(*entry, req.required_level);
        }
    }

    // Create grants - if multiple subdomains, suggest wildcard
    domain_levels
        .into_iter()
        .map(|(domain, level)| {
            // Could enhance to detect multiple subdomains and suggest *.domain
            Grant::new(GrantTarget::domain(domain), level)
        })
        .collect()
}

/// Extracts the base domain from a domain pattern.
fn extract_base_domain(pattern: &str) -> String {
    // Remove wildcard prefix if present
    pattern.strip_prefix("*.").unwrap_or(pattern).to_string()
}

/// Computes suggested grants for command-based requests.
fn compute_command_grants(requests: &[&PermissionRequest]) -> Vec<Grant> {
    if requests.is_empty() {
        return Vec::new();
    }

    // Group by command prefix (first word) and track max level
    let mut cmd_groups: HashMap<String, (PermissionLevel, Vec<String>)> = HashMap::new();

    for req in requests {
        if let GrantTarget::Command { pattern } = &req.target {
            let prefix = extract_command_prefix(pattern);
            let entry = cmd_groups
                .entry(prefix)
                .or_insert((PermissionLevel::None, Vec::new()));
            entry.0 = std::cmp::max(entry.0, req.required_level);
            entry.1.push(pattern.clone());
        }
    }

    // Create grants - if multiple commands with same prefix, suggest wildcard
    cmd_groups
        .into_iter()
        .map(|(prefix, (level, commands))| {
            let pattern = if commands.len() > 1 {
                format!("{} *", prefix)
            } else {
                commands.into_iter().next().unwrap_or(prefix)
            };
            Grant::new(GrantTarget::command(pattern), level)
        })
        .collect()
}

/// Extracts the command prefix (first word) from a command.
fn extract_command_prefix(command: &str) -> String {
    command
        .split_whitespace()
        .next()
        .unwrap_or(command)
        .to_string()
}

/// User actions for responding to a batch permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchAction {
    /// Approve all requests using the suggested grants.
    AllowAll,
    /// Approve selected requests only.
    AllowSelected,
    /// Deny all requests.
    DenyAll,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_request_creation() {
        let requests = vec![
            PermissionRequest::file_read("1", "/project/src/main.rs"),
            PermissionRequest::file_read("2", "/project/src/lib.rs"),
        ];

        let batch = BatchPermissionRequest::new("batch-1", requests);

        assert_eq!(batch.batch_id, "batch-1");
        assert_eq!(batch.len(), 2);
        assert!(!batch.suggested_grants.is_empty());
    }

    #[test]
    fn test_batch_response_all_granted() {
        let grants = vec![Grant::read_path("/project/src", true)];
        let response = BatchPermissionResponse::all_granted("batch-1", grants);

        let request = PermissionRequest::file_read("1", "/project/src/main.rs");
        assert!(response.is_granted("1", &request));
        assert!(!response.has_denials());
    }

    #[test]
    fn test_batch_response_all_denied() {
        let response =
            BatchPermissionResponse::all_denied("batch-1", vec!["1".to_string(), "2".to_string()]);

        let request = PermissionRequest::file_read("1", "/project/src/main.rs");
        assert!(!response.is_granted("1", &request));
        assert!(response.has_denials());
    }

    #[test]
    fn test_batch_response_auto_approved() {
        let response =
            BatchPermissionResponse::with_auto_approved("batch-1", vec!["1".to_string()]);

        let request = PermissionRequest::file_read("1", "/project/src/main.rs");
        assert!(response.is_granted("1", &request));
    }

    #[test]
    fn test_compute_suggested_grants_single_path() {
        let requests = vec![PermissionRequest::file_read("1", "/project/src/main.rs")];

        let grants = compute_suggested_grants(&requests);

        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].level, PermissionLevel::Read);
    }

    #[test]
    fn test_compute_suggested_grants_multiple_same_dir() {
        let requests = vec![
            PermissionRequest::file_read("1", "/project/src/main.rs"),
            PermissionRequest::file_read("2", "/project/src/lib.rs"),
        ];

        let grants = compute_suggested_grants(&requests);

        // Should suggest a single grant for the parent directory
        assert_eq!(grants.len(), 1);
        if let GrantTarget::Path { path, recursive } = &grants[0].target {
            assert_eq!(path.to_str().unwrap(), "/project/src");
            assert!(recursive); // Multiple files means recursive
        } else {
            panic!("Expected path target");
        }
    }

    #[test]
    fn test_compute_suggested_grants_different_levels() {
        let requests = vec![
            PermissionRequest::file_read("1", "/project/src/main.rs"),
            PermissionRequest::file_write("2", "/project/src/lib.rs"),
        ];

        let grants = compute_suggested_grants(&requests);

        // Should use the highest level needed
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].level, PermissionLevel::Write);
    }

    #[test]
    fn test_compute_suggested_grants_mixed_targets() {
        let requests = vec![
            PermissionRequest::file_read("1", "/project/src/main.rs"),
            PermissionRequest::command_execute("2", "git status"),
        ];

        let grants = compute_suggested_grants(&requests);

        // Should have separate grants for path and command
        assert_eq!(grants.len(), 2);
    }

    #[test]
    fn test_compute_suggested_grants_commands() {
        let requests = vec![
            PermissionRequest::command_execute("1", "git status"),
            PermissionRequest::command_execute("2", "git commit -m 'msg'"),
        ];

        let grants = compute_suggested_grants(&requests);

        // Should suggest "git *" pattern
        assert_eq!(grants.len(), 1);
        if let GrantTarget::Command { pattern } = &grants[0].target {
            assert!(pattern.contains("git"));
        } else {
            panic!("Expected command target");
        }
    }

    #[test]
    fn test_is_granted_conflict_resolution() {
        // Create a malformed response where the same ID is in both sets
        let response = BatchPermissionResponse {
            batch_id: "batch-1".to_string(),
            approved_grants: Vec::new(),
            denied_requests: ["conflict-id".to_string()].into_iter().collect(),
            auto_approved: ["conflict-id".to_string()].into_iter().collect(),
        };

        let request = PermissionRequest::file_read("conflict-id", "/project/src/main.rs");

        // Should be denied (safe default) when in both sets
        assert!(
            !response.is_granted("conflict-id", &request),
            "Conflicting request should be denied as safe default"
        );
    }
}
