//! Permission Policy - Automatic handling of permission requests
//!
//! The [`PermissionPolicy`] trait allows automatic approval, denial, or
//! filtering of permission requests before they reach the user.

use crate::permissions::{Grant, PermissionRequest};

/// Decision from a permission policy.
#[derive(Debug, Clone)]
pub enum PolicyDecision {
    /// Allow the request immediately.
    Allow,

    /// Allow and grant permission for similar future requests.
    ///
    /// The grant is stored and used to auto-approve matching requests.
    AllowWithGrant(Grant),

    /// Deny the request immediately.
    Deny {
        /// Optional reason for denial (shown to LLM).
        reason: Option<String>,
    },

    /// Defer to the consumer (show prompt to user).
    ///
    /// This is the default for interactive frontends.
    AskUser,
}

impl PolicyDecision {
    /// Create an Allow decision.
    pub fn allow() -> Self {
        Self::Allow
    }

    /// Create an AllowWithGrant decision.
    pub fn allow_with_grant(grant: Grant) -> Self {
        Self::AllowWithGrant(grant)
    }

    /// Create a Deny decision with a reason.
    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny {
            reason: Some(reason.into()),
        }
    }

    /// Create a Deny decision without a reason.
    pub fn deny_silent() -> Self {
        Self::Deny { reason: None }
    }

    /// Create an AskUser decision.
    pub fn ask_user() -> Self {
        Self::AskUser
    }
}

/// Policy for handling permission requests.
///
/// Implement this trait to automatically approve, deny, or filter
/// permission requests before they reach the user. Useful for:
///
/// - **Headless servers**: Auto-approve everything in trusted environments
/// - **Allowlists**: Only prompt for paths/commands outside a safe list
/// - **Audit logging**: Log all permission requests regardless of decision
/// - **Rate limiting**: Deny requests that exceed usage thresholds
///
/// # Example: Custom Allowlist Policy
///
/// ```ignore
/// use agent_core_runtime::agent::interface::{PermissionPolicy, PolicyDecision};
/// use agent_core_runtime::permissions::{PermissionRequest, GrantTarget};
///
/// struct AllowlistPolicy {
///     allowed_paths: Vec<String>,
/// }
///
/// impl PermissionPolicy for AllowlistPolicy {
///     fn decide(&self, request: &PermissionRequest) -> PolicyDecision {
///         match &request.target {
///             GrantTarget::Path { path, .. } => {
///                 let path_str = path.to_string_lossy();
///                 if self.allowed_paths.iter().any(|p| path_str.starts_with(p)) {
///                     PolicyDecision::Allow
///                 } else {
///                     PolicyDecision::AskUser
///                 }
///             }
///             _ => PolicyDecision::AskUser,
///         }
///     }
/// }
/// ```
pub trait PermissionPolicy: Send + Sync + 'static {
    /// Decide how to handle a permission request.
    ///
    /// Called before the request is sent to the consumer. Return:
    /// - `Allow` or `AllowWithGrant` to approve immediately
    /// - `Deny` to reject immediately
    /// - `AskUser` to forward to the consumer for user decision
    fn decide(&self, request: &PermissionRequest) -> PolicyDecision;

    /// Whether this policy supports interactive user questions.
    ///
    /// Returns `false` for headless/auto-approve policies, causing
    /// `UserInteractionRequired` events to be auto-cancelled (the tool
    /// receives "User declined to answer").
    ///
    /// Returns `true` (default) for interactive policies where a user
    /// can answer questions.
    fn supports_interaction(&self) -> bool {
        true
    }
}

/// Auto-approve all permission requests.
///
/// Use this policy in trusted environments where all tool operations
/// should be allowed without user interaction.
///
/// # Warning
///
/// This grants full access to file system, commands, network, etc.
/// Only use in controlled environments where you trust the LLM's actions.
///
/// # Example
///
/// ```ignore
/// use agent_core_runtime::agent::interface::AutoApprovePolicy;
///
/// let policy = AutoApprovePolicy;
/// // All permission requests will be automatically approved
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct AutoApprovePolicy;

impl AutoApprovePolicy {
    /// Create a new auto-approve policy.
    pub fn new() -> Self {
        Self
    }
}

impl PermissionPolicy for AutoApprovePolicy {
    fn decide(&self, _request: &PermissionRequest) -> PolicyDecision {
        PolicyDecision::Allow
    }

    fn supports_interaction(&self) -> bool {
        false // Headless - no user to answer questions
    }
}

/// Deny all permission requests.
///
/// Use this for read-only or sandboxed environments where no
/// tool operations should be allowed.
///
/// # Example
///
/// ```ignore
/// use agent_core_runtime::agent::interface::DenyAllPolicy;
///
/// let policy = DenyAllPolicy::new();
/// // All permission requests will be denied
/// ```
#[derive(Debug, Clone, Default)]
pub struct DenyAllPolicy {
    reason: Option<String>,
}

impl DenyAllPolicy {
    /// Create a new deny-all policy with a default message.
    pub fn new() -> Self {
        Self {
            reason: Some("Permission denied by policy".to_string()),
        }
    }

    /// Create a deny-all policy with a custom reason.
    pub fn with_reason(reason: impl Into<String>) -> Self {
        Self {
            reason: Some(reason.into()),
        }
    }
}

impl PermissionPolicy for DenyAllPolicy {
    fn decide(&self, _request: &PermissionRequest) -> PolicyDecision {
        PolicyDecision::Deny {
            reason: self.reason.clone(),
        }
    }
}

/// Interactive policy - always ask the user.
///
/// This is the default policy used by the TUI. All permission
/// requests are forwarded to the consumer for user decision.
///
/// # Example
///
/// ```ignore
/// use agent_core_runtime::agent::interface::InteractivePolicy;
///
/// let policy = InteractivePolicy;
/// // All permission requests will prompt the user
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct InteractivePolicy;

impl InteractivePolicy {
    /// Create a new interactive policy.
    pub fn new() -> Self {
        Self
    }
}

impl PermissionPolicy for InteractivePolicy {
    fn decide(&self, _request: &PermissionRequest) -> PolicyDecision {
        PolicyDecision::AskUser
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{GrantTarget, PermissionLevel};
    use std::path::PathBuf;

    fn make_request(target: GrantTarget, level: PermissionLevel) -> PermissionRequest {
        PermissionRequest {
            id: "test-id".to_string(),
            target,
            required_level: level,
            description: "Test operation".to_string(),
            reason: None,
            tool_name: Some("test_tool".to_string()),
        }
    }

    #[test]
    fn test_auto_approve_policy() {
        let policy = AutoApprovePolicy::new();
        let request = make_request(
            GrantTarget::Path {
                path: PathBuf::from("/etc/passwd"),
                recursive: false,
            },
            PermissionLevel::Read,
        );

        match policy.decide(&request) {
            PolicyDecision::Allow => {}
            other => panic!("Expected Allow, got {:?}", other),
        }
    }

    #[test]
    fn test_deny_all_policy() {
        let policy = DenyAllPolicy::new();
        let request = make_request(
            GrantTarget::Path {
                path: PathBuf::from("/tmp/test"),
                recursive: false,
            },
            PermissionLevel::Write,
        );

        match policy.decide(&request) {
            PolicyDecision::Deny { reason } => {
                assert!(reason.is_some());
            }
            other => panic!("Expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn test_deny_all_policy_custom_reason() {
        let policy = DenyAllPolicy::with_reason("Sandbox mode");
        let request = make_request(
            GrantTarget::Command {
                pattern: "rm".to_string(),
            },
            PermissionLevel::Execute,
        );

        match policy.decide(&request) {
            PolicyDecision::Deny { reason } => {
                assert_eq!(reason, Some("Sandbox mode".to_string()));
            }
            other => panic!("Expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn test_interactive_policy() {
        let policy = InteractivePolicy::new();
        let request = make_request(
            GrantTarget::Domain {
                pattern: "api.example.com".to_string(),
            },
            PermissionLevel::Read,
        );

        match policy.decide(&request) {
            PolicyDecision::AskUser => {}
            other => panic!("Expected AskUser, got {:?}", other),
        }
    }

    #[test]
    fn test_policy_decision_constructors() {
        let allow = PolicyDecision::allow();
        assert!(matches!(allow, PolicyDecision::Allow));

        let deny = PolicyDecision::deny("test reason");
        assert!(matches!(deny, PolicyDecision::Deny { reason: Some(_) }));

        let deny_silent = PolicyDecision::deny_silent();
        assert!(matches!(deny_silent, PolicyDecision::Deny { reason: None }));

        let ask = PolicyDecision::ask_user();
        assert!(matches!(ask, PolicyDecision::AskUser));
    }
}
