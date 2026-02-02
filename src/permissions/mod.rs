//! Permission system for controlling agent access to resources.
//!
//! This module implements a grant-based permission system where permissions are
//! represented as tuples of `(Target, Level)`:
//!
//! - **Target**: What resource is being accessed (path, domain, or command)
//! - **Level**: What level of access is permitted (read, write, execute, admin)
//!
//! ## Permission Levels
//!
//! Permission levels form a hierarchy where higher levels imply lower levels:
//!
//! ```text
//! Admin > Execute > Write > Read > None
//! ```
//!
//! For example, a `Write` grant automatically allows `Read` operations.
//!
//! ## Target Types
//!
//! | Target | Controls | Examples |
//! |--------|----------|----------|
//! | `Path` | Files and directories | `/project/src`, `/home/user/.config` |
//! | `Domain` | Network endpoints | `api.github.com`, `*.anthropic.com` |
//! | `Command` | Shell commands | `git *`, `cargo build` |
//!
//! ## Batch Requests
//!
//! When multiple tools run in parallel, their permission requests can be
//! batched together for a single UI prompt, avoiding deadlocks and reducing
//! user friction.
//!
//! ## Example
//!
//! ```
//! use agent_core::permissions::{Grant, GrantTarget, PermissionLevel, PermissionRequest};
//!
//! // Create a grant for writing to /project/src recursively
//! let grant = Grant::write_path("/project/src", true);
//!
//! // Create a request to write a file
//! let request = PermissionRequest::file_write("req-1", "/project/src/main.rs");
//!
//! // Check if the grant satisfies the request
//! assert!(grant.satisfies(&request));
//!
//! // Write grant also satisfies read requests (level hierarchy)
//! let read_request = PermissionRequest::file_read("req-2", "/project/src/lib.rs");
//! assert!(grant.satisfies(&read_request));
//! ```

mod batch;
mod grant;
mod level;
mod registry;
mod target;
mod tool_mapping;

pub use batch::{
    compute_suggested_grants, BatchAction, BatchPermissionRequest, BatchPermissionResponse,
};
pub use grant::{Grant, PermissionRequest};
pub use level::PermissionLevel;
pub use registry::{
    generate_batch_id, PendingPermissionInfo, PermissionError, PermissionPanelResponse,
    PermissionRegistry,
};
pub use target::GrantTarget;
pub use tool_mapping::{get_tool_category, ToolCategory, ToolPermissions};

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_end_to_end_permission_check() {
        // Simulate granting read access to a project
        let grant = Grant::read_path("/project", true);

        // Various read requests should be satisfied
        let requests = vec![
            PermissionRequest::file_read("1", "/project/src/main.rs"),
            PermissionRequest::file_read("2", "/project/Cargo.toml"),
            PermissionRequest::directory_read("3", "/project/src", true),
        ];

        for request in &requests {
            assert!(
                grant.satisfies(request),
                "Grant should satisfy request: {}",
                request.id
            );
        }

        // Write request should NOT be satisfied
        let write_request = PermissionRequest::file_write("4", "/project/src/main.rs");
        assert!(
            !grant.satisfies(&write_request),
            "Read grant should not satisfy write request"
        );

        // Request outside the grant path should NOT be satisfied
        let outside_request = PermissionRequest::file_read("5", "/other/file.rs");
        assert!(
            !grant.satisfies(&outside_request),
            "Grant should not satisfy request outside path"
        );
    }

    #[test]
    fn test_level_hierarchy() {
        // Admin grant should satisfy all levels
        let admin_grant = Grant::admin_path("/project", true);

        let requests = vec![
            PermissionRequest::file_read("1", "/project/file.rs"),
            PermissionRequest::file_write("2", "/project/file.rs"),
            PermissionRequest::new(
                "3",
                GrantTarget::path("/project/file.rs", false),
                PermissionLevel::Execute,
                "Execute",
            ),
            PermissionRequest::new(
                "4",
                GrantTarget::path("/project/file.rs", false),
                PermissionLevel::Admin,
                "Admin",
            ),
        ];

        for request in &requests {
            assert!(
                admin_grant.satisfies(request),
                "Admin grant should satisfy {} level request",
                request.required_level
            );
        }
    }

    #[test]
    fn test_batch_permission_flow() {
        // Simulate parallel tool execution requesting permissions
        let requests = vec![
            PermissionRequest::file_read("tool-1", "/project/src/main.rs"),
            PermissionRequest::file_read("tool-2", "/project/src/lib.rs"),
            PermissionRequest::file_read("tool-3", "/project/Cargo.toml"),
        ];

        // Create batch request
        let batch = BatchPermissionRequest::new("batch-1", requests.clone());

        // Verify suggestions were computed
        assert!(!batch.suggested_grants.is_empty());

        // Simulate user approving with a recursive grant
        let approved_grant = Grant::read_path("/project", true);
        let response = BatchPermissionResponse::all_granted("batch-1", vec![approved_grant]);

        // All requests should now be satisfied
        for request in &requests {
            assert!(
                response.is_granted(&request.id, request),
                "Request {} should be granted",
                request.id
            );
        }
    }

    #[test]
    fn test_different_target_types_independent() {
        // Path grant should not satisfy domain request
        let path_grant = Grant::read_path("/project", true);
        let domain_request =
            PermissionRequest::network_access("1", "api.github.com", PermissionLevel::Read);
        assert!(!path_grant.satisfies(&domain_request));

        // Domain grant should not satisfy command request
        let domain_grant = Grant::domain("*", PermissionLevel::Admin);
        let cmd_request = PermissionRequest::command_execute("2", "git status");
        assert!(!domain_grant.satisfies(&cmd_request));

        // Command grant should not satisfy path request
        let cmd_grant = Grant::command("*", PermissionLevel::Admin);
        let path_request = PermissionRequest::file_read("3", "/project/file.rs");
        assert!(!cmd_grant.satisfies(&path_request));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let grant = Grant::write_path("/project/src", true);
        let json = serde_json::to_string(&grant).unwrap();
        let deserialized: Grant = serde_json::from_str(&json).unwrap();

        assert_eq!(grant.level, deserialized.level);
        assert_eq!(grant.target, deserialized.target);

        let request = PermissionRequest::file_write("test", "/project/src/main.rs")
            .with_reason("Testing")
            .with_tool("write_file");
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: PermissionRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.id, deserialized.id);
        assert_eq!(request.reason, deserialized.reason);
        assert_eq!(request.tool_name, deserialized.tool_name);
    }
}
