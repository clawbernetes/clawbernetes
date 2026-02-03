//! Access control for secrets.
//!
//! This module provides policy-based access control for secrets,
//! evaluating whether a given accessor is allowed to access a secret
//! based on its access policy.

use crate::audit::AuditLog;
use crate::error::{Error, Result};
use crate::types::{AccessPolicy, Accessor, AuditAction, AuditEntry, SecretId};

/// Controller for evaluating access policies.
///
/// The access controller checks whether an accessor is allowed to
/// access a secret based on its policy, and records the access
/// attempt in the audit log.
pub struct AccessController {
    audit_log: AuditLog,
}

impl AccessController {
    /// Creates a new access controller with the given audit log.
    #[must_use]
    pub const fn new(audit_log: AuditLog) -> Self {
        Self { audit_log }
    }

    /// Creates a new access controller with a fresh audit log.
    #[must_use]
    pub fn with_new_audit_log() -> Self {
        Self::new(AuditLog::new())
    }

    /// Returns a reference to the audit log.
    #[must_use]
    pub const fn audit_log(&self) -> &AuditLog {
        &self.audit_log
    }

    /// Checks if an accessor is allowed to access a secret.
    ///
    /// This method evaluates the access policy and records the access
    /// attempt in the audit log.
    ///
    /// # Arguments
    ///
    /// * `secret_id` - The identifier of the secret being accessed
    /// * `policy` - The access policy for the secret
    /// * `accessor` - Who is attempting to access the secret
    /// * `reason` - Human-readable reason for the access
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The policy has expired
    /// - The accessor is not allowed by the policy
    pub fn check(
        &self,
        secret_id: &SecretId,
        policy: &AccessPolicy,
        accessor: &Accessor,
        reason: &str,
    ) -> Result<()> {
        // Check if policy has expired
        if policy.is_expired() {
            self.record_denied(secret_id, accessor, reason, "policy expired");
            return Err(Error::AccessDenied {
                reason: "access policy has expired".to_string(),
            });
        }

        // Check if accessor is allowed
        let allowed = match accessor {
            Accessor::Workload(workload_id) => policy.allows_workload(workload_id),
            Accessor::Node(node_id) => policy.allows_node(node_id),
            Accessor::Admin(_) => true, // Admins always have access
            Accessor::System => true,   // System always has access
        };

        if !allowed {
            self.record_denied(secret_id, accessor, reason, "accessor not in policy");
            return Err(Error::AccessDenied {
                reason: format!("accessor {accessor} is not allowed by policy"),
            });
        }

        // Record successful access check
        self.audit_log.record(AuditEntry::new(
            secret_id.clone(),
            accessor.clone(),
            AuditAction::Read,
            reason,
        ));

        Ok(())
    }

    /// Records a denied access attempt.
    fn record_denied(
        &self,
        secret_id: &SecretId,
        accessor: &Accessor,
        reason: &str,
        denial_reason: &str,
    ) {
        let full_reason = format!("{reason} (denied: {denial_reason})");
        self.audit_log.record(AuditEntry::new(
            secret_id.clone(),
            accessor.clone(),
            AuditAction::AccessDenied,
            full_reason,
        ));
    }

    /// Records a secret creation event.
    pub fn record_created(&self, secret_id: &SecretId, accessor: &Accessor, reason: &str) {
        self.audit_log.record(AuditEntry::new(
            secret_id.clone(),
            accessor.clone(),
            AuditAction::Created,
            reason,
        ));
    }

    /// Records a secret update event.
    pub fn record_updated(&self, secret_id: &SecretId, accessor: &Accessor, reason: &str) {
        self.audit_log.record(AuditEntry::new(
            secret_id.clone(),
            accessor.clone(),
            AuditAction::Updated,
            reason,
        ));
    }

    /// Records a secret deletion event.
    pub fn record_deleted(&self, secret_id: &SecretId, accessor: &Accessor, reason: &str) {
        self.audit_log.record(AuditEntry::new(
            secret_id.clone(),
            accessor.clone(),
            AuditAction::Deleted,
            reason,
        ));
    }

    /// Records a secret rotation event.
    pub fn record_rotated(&self, secret_id: &SecretId, accessor: &Accessor, reason: &str) {
        self.audit_log.record(AuditEntry::new(
            secret_id.clone(),
            accessor.clone(),
            AuditAction::Rotated,
            reason,
        ));
    }
}

impl std::fmt::Debug for AccessController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccessController")
            .field("audit_log", &self.audit_log)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::AuditFilter;
    use crate::types::{NodeId, WorkloadId};
    use chrono::{Duration, Utc};

    fn test_controller() -> AccessController {
        AccessController::with_new_audit_log()
    }

    fn test_secret_id() -> SecretId {
        SecretId::new("test-secret").expect("valid id")
    }

    #[test]
    fn access_controller_new() {
        let controller = test_controller();
        assert!(controller.audit_log().is_empty());
    }

    #[test]
    fn access_check_system_always_allowed() {
        let controller = test_controller();
        let secret_id = test_secret_id();
        let policy = AccessPolicy::new(); // Empty policy

        let result = controller.check(&secret_id, &policy, &Accessor::System, "system access");

        assert!(result.is_ok());
        assert_eq!(controller.audit_log().len(), 1);
    }

    #[test]
    fn access_check_admin_always_allowed() {
        let controller = test_controller();
        let secret_id = test_secret_id();
        let policy = AccessPolicy::new(); // Empty policy

        let accessor = Accessor::Admin("alice".to_string());
        let result = controller.check(&secret_id, &policy, &accessor, "admin access");

        assert!(result.is_ok());
    }

    #[test]
    fn access_check_workload_allowed() {
        let controller = test_controller();
        let secret_id = test_secret_id();
        let workload = WorkloadId::new("my-workload");
        let policy = AccessPolicy::allow_workloads(vec![workload.clone()]);

        let accessor = Accessor::Workload(workload);
        let result = controller.check(&secret_id, &policy, &accessor, "routine access");

        assert!(result.is_ok());
    }

    #[test]
    fn access_check_workload_denied() {
        let controller = test_controller();
        let secret_id = test_secret_id();
        let allowed = WorkloadId::new("allowed-workload");
        let denied = WorkloadId::new("denied-workload");
        let policy = AccessPolicy::allow_workloads(vec![allowed]);

        let accessor = Accessor::Workload(denied);
        let result = controller.check(&secret_id, &policy, &accessor, "attempted access");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::AccessDenied { .. }));

        // Check audit log recorded the denial
        let entries = controller.audit_log().query(&AuditFilter::new());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, AuditAction::AccessDenied);
    }

    #[test]
    fn access_check_node_allowed() {
        let controller = test_controller();
        let secret_id = test_secret_id();
        let node = NodeId::new("my-node");
        let policy = AccessPolicy::allow_nodes(vec![node.clone()]);

        let accessor = Accessor::Node(node);
        let result = controller.check(&secret_id, &policy, &accessor, "node access");

        assert!(result.is_ok());
    }

    #[test]
    fn access_check_node_denied() {
        let controller = test_controller();
        let secret_id = test_secret_id();
        let allowed = NodeId::new("allowed-node");
        let denied = NodeId::new("denied-node");
        let policy = AccessPolicy::allow_nodes(vec![allowed]);

        let accessor = Accessor::Node(denied);
        let result = controller.check(&secret_id, &policy, &accessor, "attempted access");

        assert!(result.is_err());
    }

    #[test]
    fn access_check_expired_policy() {
        let controller = test_controller();
        let secret_id = test_secret_id();
        let workload = WorkloadId::new("my-workload");

        // Create a policy that expired an hour ago
        let past = Utc::now() - Duration::hours(1);
        let policy = AccessPolicy::allow_workloads(vec![workload.clone()]).with_expiry(past);

        let accessor = Accessor::Workload(workload);
        let result = controller.check(&secret_id, &policy, &accessor, "access attempt");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::AccessDenied { .. }));
        assert!(err.to_string().contains("expired"));
    }

    #[test]
    fn access_check_future_expiry_allowed() {
        let controller = test_controller();
        let secret_id = test_secret_id();
        let workload = WorkloadId::new("my-workload");

        // Create a policy that expires in the future
        let future = Utc::now() + Duration::hours(1);
        let policy = AccessPolicy::allow_workloads(vec![workload.clone()]).with_expiry(future);

        let accessor = Accessor::Workload(workload);
        let result = controller.check(&secret_id, &policy, &accessor, "access");

        assert!(result.is_ok());
    }

    #[test]
    fn record_created() {
        let controller = test_controller();
        let secret_id = test_secret_id();

        controller.record_created(&secret_id, &Accessor::System, "initial creation");

        let entries = controller.audit_log().query(&AuditFilter::new());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, AuditAction::Created);
    }

    #[test]
    fn record_updated() {
        let controller = test_controller();
        let secret_id = test_secret_id();

        controller.record_updated(&secret_id, &Accessor::System, "value update");

        let entries = controller.audit_log().query(&AuditFilter::new());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, AuditAction::Updated);
    }

    #[test]
    fn record_deleted() {
        let controller = test_controller();
        let secret_id = test_secret_id();

        controller.record_deleted(&secret_id, &Accessor::System, "cleanup");

        let entries = controller.audit_log().query(&AuditFilter::new());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, AuditAction::Deleted);
    }

    #[test]
    fn record_rotated() {
        let controller = test_controller();
        let secret_id = test_secret_id();

        controller.record_rotated(&secret_id, &Accessor::System, "scheduled rotation");

        let entries = controller.audit_log().query(&AuditFilter::new());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, AuditAction::Rotated);
    }

    #[test]
    fn access_denied_records_reason() {
        let controller = test_controller();
        let secret_id = test_secret_id();
        let policy = AccessPolicy::new(); // Empty policy - denies all workloads

        let accessor = Accessor::Workload(WorkloadId::new("unauthorized"));
        let _ = controller.check(&secret_id, &policy, &accessor, "suspicious access");

        let entries = controller.audit_log().query(&AuditFilter::new());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].reason.contains("suspicious access"));
        assert!(entries[0].reason.contains("denied"));
    }

    #[test]
    fn multiple_access_checks_logged() {
        let controller = test_controller();
        let secret_id = test_secret_id();
        let workload = WorkloadId::new("my-workload");
        let policy = AccessPolicy::allow_workloads(vec![workload.clone()]);

        let accessor = Accessor::Workload(workload);

        // Multiple access checks
        for i in 0..5 {
            let result =
                controller.check(&secret_id, &policy, &accessor, &format!("access #{i}"));
            assert!(result.is_ok());
        }

        assert_eq!(controller.audit_log().len(), 5);
    }

    #[test]
    fn access_check_mixed_policy() {
        let controller = test_controller();
        let secret_id = test_secret_id();

        let workload = WorkloadId::new("api-server");
        let node = NodeId::new("node-1");

        let policy = AccessPolicy {
            allowed_workloads: vec![workload.clone()],
            allowed_nodes: vec![node.clone()],
            expires_at: None,
        };

        // Workload should be allowed
        let result = controller.check(
            &secret_id,
            &policy,
            &Accessor::Workload(workload),
            "workload access",
        );
        assert!(result.is_ok());

        // Node should be allowed
        let result =
            controller.check(&secret_id, &policy, &Accessor::Node(node), "node access");
        assert!(result.is_ok());

        // Other workload should be denied
        let result = controller.check(
            &secret_id,
            &policy,
            &Accessor::Workload(WorkloadId::new("other")),
            "other access",
        );
        assert!(result.is_err());
    }
}
