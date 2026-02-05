//! Role-Based Access Control (RBAC) policy evaluation.
//!
//! This module provides RBAC policy management and evaluation:
//! - [`RbacPolicy`]: A complete RBAC policy with roles and user assignments
//! - [`AuthContext`]: The authenticated context for permission checks
//! - Policy evaluation functions

use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::types::{Action, Permission, Role, Scope, User, UserId};

/// RBAC policy that maps roles to permissions and users to roles.
#[derive(Debug, Default)]
pub struct RbacPolicy {
    /// Defined roles.
    roles: HashMap<String, Role>,
    /// Users and their data.
    users: HashMap<UserId, User>,
}

impl RbacPolicy {
    /// Creates a new empty RBAC policy.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a policy with default roles (admin, operator, readonly).
    #[must_use]
    pub fn with_default_roles() -> Self {
        let mut policy = Self::new();
        policy.add_role(Role::admin());
        policy.add_role(Role::operator());
        policy.add_role(Role::readonly());
        policy
    }

    /// Adds a role to the policy.
    pub fn add_role(&mut self, role: Role) {
        self.roles.insert(role.name.clone(), role);
    }

    /// Gets a role by name.
    #[must_use]
    pub fn get_role(&self, name: &str) -> Option<&Role> {
        self.roles.get(name)
    }

    /// Gets a mutable reference to a role by name.
    #[must_use]
    pub fn get_role_mut(&mut self, name: &str) -> Option<&mut Role> {
        self.roles.get_mut(name)
    }

    /// Removes a role from the policy.
    pub fn remove_role(&mut self, name: &str) -> Option<Role> {
        self.roles.remove(name)
    }

    /// Lists all roles.
    #[must_use]
    pub fn list_roles(&self) -> Vec<&Role> {
        self.roles.values().collect()
    }

    /// Adds a user to the policy.
    pub fn add_user(&mut self, user: User) {
        self.users.insert(user.id.clone(), user);
    }

    /// Gets a user by ID.
    #[must_use]
    pub fn get_user(&self, id: &UserId) -> Option<&User> {
        self.users.get(id)
    }

    /// Gets a mutable reference to a user by ID.
    #[must_use]
    pub fn get_user_mut(&mut self, id: &UserId) -> Option<&mut User> {
        self.users.get_mut(id)
    }

    /// Removes a user from the policy.
    pub fn remove_user(&mut self, id: &UserId) -> Option<User> {
        self.users.remove(id)
    }

    /// Lists all users.
    #[must_use]
    pub fn list_users(&self) -> Vec<&User> {
        self.users.values().collect()
    }

    /// Assigns a role to a user.
    ///
    /// # Errors
    ///
    /// Returns an error if the user or role doesn't exist.
    pub fn assign_role(&mut self, user_id: &UserId, role_name: &str) -> Result<()> {
        if !self.roles.contains_key(role_name) {
            return Err(Error::RoleNotFound {
                name: role_name.to_string(),
            });
        }
        let user = self.users.get_mut(user_id).ok_or_else(|| Error::UserNotFound {
            id: user_id.to_string(),
        })?;
        user.add_role(role_name);
        Ok(())
    }

    /// Removes a role from a user.
    ///
    /// # Errors
    ///
    /// Returns an error if the user doesn't exist.
    pub fn unassign_role(&mut self, user_id: &UserId, role_name: &str) -> Result<()> {
        let user = self.users.get_mut(user_id).ok_or_else(|| Error::UserNotFound {
            id: user_id.to_string(),
        })?;
        user.remove_role(role_name);
        Ok(())
    }

    /// Gets all permissions for a user based on their roles.
    #[must_use]
    pub fn get_user_permissions(&self, user_id: &UserId) -> Vec<&Permission> {
        let Some(user) = self.users.get(user_id) else {
            return Vec::new();
        };

        user.roles
            .iter()
            .filter_map(|role_name| self.roles.get(role_name))
            .flat_map(|role| role.permissions.iter())
            .collect()
    }

    /// Checks if a user has permission for the given resource and action.
    #[must_use]
    pub fn check_permission(&self, user_id: &UserId, resource: &str, action: Action) -> bool {
        let Some(user) = self.users.get(user_id) else {
            return false;
        };

        if !user.active {
            return false;
        }

        user.roles.iter().any(|role_name| {
            self.roles
                .get(role_name)
                .is_some_and(|role| role.has_permission(resource, action))
        })
    }

    /// Evaluates a permission check and returns a Result.
    ///
    /// # Errors
    ///
    /// Returns an error if the user doesn't exist or lacks the required permission.
    pub fn evaluate(&self, user_id: &UserId, resource: &str, action: Action) -> Result<()> {
        let user = self.users.get(user_id).ok_or_else(|| Error::UserNotFound {
            id: user_id.to_string(),
        })?;

        if !user.active {
            return Err(Error::PermissionDenied {
                reason: "user is deactivated".to_string(),
            });
        }

        if self.check_permission(user_id, resource, action) {
            Ok(())
        } else {
            Err(Error::PermissionDenied {
                reason: format!(
                    "user '{}' lacks permission for {}:{} on resource '{}'",
                    user.name, resource, action, resource
                ),
            })
        }
    }
}

/// Authentication context containing the current user and their permissions.
///
/// This is the primary interface for checking permissions in request handlers.
#[derive(Debug)]
pub struct AuthContext {
    /// The authenticated user ID.
    user_id: UserId,
    /// User's roles.
    roles: Vec<String>,
    /// Token scopes (if authenticated via token/API key).
    scopes: Vec<Scope>,
    /// Whether the context is authenticated.
    authenticated: bool,
}

impl AuthContext {
    /// Creates a new authenticated context for a user.
    #[must_use]
    pub fn authenticated(user_id: UserId, roles: Vec<String>) -> Self {
        Self {
            user_id,
            roles,
            scopes: Vec::new(),
            authenticated: true,
        }
    }

    /// Creates an unauthenticated (anonymous) context.
    #[must_use]
    pub fn anonymous() -> Self {
        Self {
            user_id: UserId::anonymous(),
            roles: Vec::new(),
            scopes: Vec::new(),
            authenticated: false,
        }
    }

    /// Adds scopes to the context (from API key or token).
    #[must_use]
    pub fn with_scopes(mut self, scopes: Vec<Scope>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Returns the user ID.
    #[must_use]
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    /// Returns the user's roles.
    #[must_use]
    pub fn roles(&self) -> &[String] {
        &self.roles
    }

    /// Returns the scopes.
    #[must_use]
    pub fn scopes(&self) -> &[Scope] {
        &self.scopes
    }

    /// Returns true if the context is authenticated.
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    /// Checks if the user has a specific role.
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Checks if the context has a scope that allows the action.
    #[must_use]
    pub fn has_scope(&self, resource: &str, action: Action) -> bool {
        self.scopes.iter().any(|s| s.allows(resource, action))
    }

    /// Checks if this context can perform the action on the resource.
    ///
    /// This checks:
    /// 1. If authenticated
    /// 2. If scopes (if any) allow the action
    ///
    /// Note: Role-based permissions should be checked via `RbacPolicy::check_permission`.
    #[must_use]
    pub fn can(&self, resource: &str, action: Action) -> bool {
        if !self.authenticated {
            return false;
        }

        // If scopes are present, check them
        if !self.scopes.is_empty() {
            return self.has_scope(resource, action);
        }

        // If no scopes, allow (role check is done separately via RbacPolicy)
        true
    }

    /// Requires authentication.
    ///
    /// # Errors
    ///
    /// Returns an error if not authenticated.
    pub fn require_auth(&self) -> Result<()> {
        if self.authenticated {
            Ok(())
        } else {
            Err(Error::AuthenticationRequired)
        }
    }

    /// Requires a specific role.
    ///
    /// # Errors
    ///
    /// Returns an error if the user doesn't have the required role.
    pub fn require_role(&self, role: &str) -> Result<()> {
        self.require_auth()?;
        if self.has_role(role) {
            Ok(())
        } else {
            Err(Error::PermissionDenied {
                reason: format!("required role: {role}"),
            })
        }
    }

    /// Requires one of the specified roles.
    ///
    /// # Errors
    ///
    /// Returns an error if the user doesn't have any of the required roles.
    pub fn require_any_role(&self, roles: &[&str]) -> Result<()> {
        self.require_auth()?;
        if roles.iter().any(|r| self.has_role(r)) {
            Ok(())
        } else {
            Err(Error::PermissionDenied {
                reason: format!("required one of roles: {}", roles.join(", ")),
            })
        }
    }

    /// Requires a scope that allows the action.
    ///
    /// # Errors
    ///
    /// Returns an error if the context doesn't have the required scope.
    pub fn require_scope(&self, resource: &str, action: Action) -> Result<()> {
        self.require_auth()?;
        if self.scopes.is_empty() {
            // No scopes = no restriction from scopes
            return Ok(());
        }
        if self.has_scope(resource, action) {
            Ok(())
        } else {
            Err(Error::InsufficientScope {
                required: format!("{resource}:{action}"),
                actual: self
                    .scopes
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
            })
        }
    }

    /// Requires permission using the RBAC policy.
    ///
    /// # Errors
    ///
    /// Returns an error if the user lacks the required permission.
    pub fn require_permission(
        &self,
        policy: &RbacPolicy,
        resource: &str,
        action: Action,
    ) -> Result<()> {
        self.require_auth()?;
        self.require_scope(resource, action)?;
        policy.evaluate(&self.user_id, resource, action)
    }
}

/// Creates an auth context from a user in the RBAC policy.
///
/// # Errors
///
/// Returns an error if the user doesn't exist.
pub fn context_for_user(policy: &RbacPolicy, user_id: &UserId) -> Result<AuthContext> {
    let user = policy.get_user(user_id).ok_or_else(|| Error::UserNotFound {
        id: user_id.to_string(),
    })?;

    if !user.active {
        return Err(Error::PermissionDenied {
            reason: "user is deactivated".to_string(),
        });
    }

    Ok(AuthContext::authenticated(user_id.clone(), user.roles.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_policy() -> (RbacPolicy, UserId, UserId) {
        let mut policy = RbacPolicy::with_default_roles();

        // Create admin user
        let admin_id = UserId::new();
        let mut admin = User::with_id(admin_id.clone(), "admin-user");
        admin.add_role("admin");
        policy.add_user(admin);

        // Create operator user
        let operator_id = UserId::new();
        let mut operator = User::with_id(operator_id.clone(), "operator-user");
        operator.add_role("operator");
        policy.add_user(operator);

        (policy, admin_id, operator_id)
    }

    // ===================
    // RbacPolicy Tests
    // ===================

    #[test]
    fn rbac_policy_new() {
        let policy = RbacPolicy::new();
        assert!(policy.list_roles().is_empty());
        assert!(policy.list_users().is_empty());
    }

    #[test]
    fn rbac_policy_with_default_roles() {
        let policy = RbacPolicy::with_default_roles();
        assert!(policy.get_role("admin").is_some());
        assert!(policy.get_role("operator").is_some());
        assert!(policy.get_role("readonly").is_some());
    }

    #[test]
    fn rbac_policy_add_role() {
        let mut policy = RbacPolicy::new();
        let role = Role::new("custom", "Custom role").ok().unwrap();
        policy.add_role(role);
        assert!(policy.get_role("custom").is_some());
    }

    #[test]
    fn rbac_policy_remove_role() {
        let mut policy = RbacPolicy::with_default_roles();
        let removed = policy.remove_role("readonly");
        assert!(removed.is_some());
        assert!(policy.get_role("readonly").is_none());
    }

    #[test]
    fn rbac_policy_add_user() {
        let mut policy = RbacPolicy::new();
        let user = User::new("alice").ok().unwrap();
        let user_id = user.id.clone();
        policy.add_user(user);
        assert!(policy.get_user(&user_id).is_some());
    }

    #[test]
    fn rbac_policy_remove_user() {
        let mut policy = RbacPolicy::new();
        let user = User::new("alice").ok().unwrap();
        let user_id = user.id.clone();
        policy.add_user(user);
        let removed = policy.remove_user(&user_id);
        assert!(removed.is_some());
        assert!(policy.get_user(&user_id).is_none());
    }

    #[test]
    fn rbac_policy_assign_role() {
        let (mut policy, admin_id, _) = setup_policy();
        let user = User::new("newuser").ok().unwrap();
        let user_id = user.id.clone();
        policy.add_user(user);
        let result = policy.assign_role(&user_id, "readonly");
        assert!(result.is_ok());
        let user = policy.get_user(&user_id).unwrap();
        assert!(user.has_role("readonly"));
        // Admin should still have admin role
        let admin = policy.get_user(&admin_id).unwrap();
        assert!(admin.has_role("admin"));
    }

    #[test]
    fn rbac_policy_assign_nonexistent_role() {
        let (mut policy, _, operator_id) = setup_policy();
        let result = policy.assign_role(&operator_id, "nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::RoleNotFound { .. })));
    }

    #[test]
    fn rbac_policy_assign_role_nonexistent_user() {
        let mut policy = RbacPolicy::with_default_roles();
        let fake_id = UserId::new();
        let result = policy.assign_role(&fake_id, "admin");
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::UserNotFound { .. })));
    }

    #[test]
    fn rbac_policy_unassign_role() {
        let (mut policy, admin_id, _) = setup_policy();
        let result = policy.unassign_role(&admin_id, "admin");
        assert!(result.is_ok());
        let user = policy.get_user(&admin_id).unwrap();
        assert!(!user.has_role("admin"));
    }

    #[test]
    fn rbac_policy_check_permission_admin() {
        let (policy, admin_id, _) = setup_policy();
        assert!(policy.check_permission(&admin_id, "workloads", Action::Create));
        assert!(policy.check_permission(&admin_id, "workloads", Action::Delete));
        assert!(policy.check_permission(&admin_id, "nodes", Action::Admin));
        assert!(policy.check_permission(&admin_id, "anything", Action::Execute));
    }

    #[test]
    fn rbac_policy_check_permission_operator() {
        let (policy, _, operator_id) = setup_policy();
        assert!(policy.check_permission(&operator_id, "workloads", Action::Create));
        assert!(policy.check_permission(&operator_id, "workloads", Action::Delete));
        assert!(policy.check_permission(&operator_id, "nodes", Action::Read));
        assert!(!policy.check_permission(&operator_id, "nodes", Action::Delete));
    }

    #[test]
    fn rbac_policy_check_permission_inactive_user() {
        let (mut policy, admin_id, _) = setup_policy();
        let user = policy.get_user_mut(&admin_id).unwrap();
        user.deactivate();
        assert!(!policy.check_permission(&admin_id, "workloads", Action::Create));
    }

    #[test]
    fn rbac_policy_check_permission_nonexistent_user() {
        let policy = RbacPolicy::with_default_roles();
        let fake_id = UserId::new();
        assert!(!policy.check_permission(&fake_id, "workloads", Action::Create));
    }

    #[test]
    fn rbac_policy_evaluate_success() {
        let (policy, admin_id, _) = setup_policy();
        let result = policy.evaluate(&admin_id, "workloads", Action::Create);
        assert!(result.is_ok());
    }

    #[test]
    fn rbac_policy_evaluate_denied() {
        let (policy, _, operator_id) = setup_policy();
        let result = policy.evaluate(&operator_id, "nodes", Action::Delete);
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::PermissionDenied { .. })));
    }

    #[test]
    fn rbac_policy_get_user_permissions() {
        let (policy, admin_id, _) = setup_policy();
        let permissions = policy.get_user_permissions(&admin_id);
        assert!(!permissions.is_empty());
    }

    // ===================
    // AuthContext Tests
    // ===================

    #[test]
    fn auth_context_authenticated() {
        let user_id = UserId::new();
        let ctx = AuthContext::authenticated(user_id.clone(), vec!["admin".to_string()]);
        assert!(ctx.is_authenticated());
        assert_eq!(ctx.user_id(), &user_id);
        assert!(ctx.has_role("admin"));
    }

    #[test]
    fn auth_context_anonymous() {
        let ctx = AuthContext::anonymous();
        assert!(!ctx.is_authenticated());
    }

    #[test]
    fn auth_context_with_scopes() {
        let user_id = UserId::new();
        let scopes = vec![Scope::new("workloads:read").ok().unwrap()];
        let ctx = AuthContext::authenticated(user_id, vec![]).with_scopes(scopes);
        assert!(ctx.has_scope("workloads", Action::Read));
        assert!(!ctx.has_scope("workloads", Action::Create));
    }

    #[test]
    fn auth_context_can_authenticated() {
        let user_id = UserId::new();
        let ctx = AuthContext::authenticated(user_id, vec![]);
        assert!(ctx.can("workloads", Action::Read));
    }

    #[test]
    fn auth_context_can_anonymous() {
        let ctx = AuthContext::anonymous();
        assert!(!ctx.can("workloads", Action::Read));
    }

    #[test]
    fn auth_context_can_with_scopes() {
        let user_id = UserId::new();
        let scopes = vec![Scope::new("workloads:read").ok().unwrap()];
        let ctx = AuthContext::authenticated(user_id, vec![]).with_scopes(scopes);
        assert!(ctx.can("workloads", Action::Read));
        assert!(!ctx.can("workloads", Action::Create));
    }

    #[test]
    fn auth_context_require_auth_success() {
        let user_id = UserId::new();
        let ctx = AuthContext::authenticated(user_id, vec![]);
        assert!(ctx.require_auth().is_ok());
    }

    #[test]
    fn auth_context_require_auth_failure() {
        let ctx = AuthContext::anonymous();
        let result = ctx.require_auth();
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::AuthenticationRequired)));
    }

    #[test]
    fn auth_context_require_role_success() {
        let user_id = UserId::new();
        let ctx = AuthContext::authenticated(user_id, vec!["admin".to_string()]);
        assert!(ctx.require_role("admin").is_ok());
    }

    #[test]
    fn auth_context_require_role_failure() {
        let user_id = UserId::new();
        let ctx = AuthContext::authenticated(user_id, vec!["operator".to_string()]);
        let result = ctx.require_role("admin");
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::PermissionDenied { .. })));
    }

    #[test]
    fn auth_context_require_any_role_success() {
        let user_id = UserId::new();
        let ctx = AuthContext::authenticated(user_id, vec!["operator".to_string()]);
        assert!(ctx.require_any_role(&["admin", "operator"]).is_ok());
    }

    #[test]
    fn auth_context_require_any_role_failure() {
        let user_id = UserId::new();
        let ctx = AuthContext::authenticated(user_id, vec!["readonly".to_string()]);
        let result = ctx.require_any_role(&["admin", "operator"]);
        assert!(result.is_err());
    }

    #[test]
    fn auth_context_require_scope_success() {
        let user_id = UserId::new();
        let scopes = vec![Scope::new("workloads:*").ok().unwrap()];
        let ctx = AuthContext::authenticated(user_id, vec![]).with_scopes(scopes);
        assert!(ctx.require_scope("workloads", Action::Create).is_ok());
    }

    #[test]
    fn auth_context_require_scope_failure() {
        let user_id = UserId::new();
        let scopes = vec![Scope::new("workloads:read").ok().unwrap()];
        let ctx = AuthContext::authenticated(user_id, vec![]).with_scopes(scopes);
        let result = ctx.require_scope("workloads", Action::Create);
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::InsufficientScope { .. })));
    }

    #[test]
    fn auth_context_require_scope_no_scopes() {
        let user_id = UserId::new();
        let ctx = AuthContext::authenticated(user_id, vec![]);
        // No scopes means no scope restriction
        assert!(ctx.require_scope("workloads", Action::Create).is_ok());
    }

    #[test]
    fn auth_context_require_permission() {
        let (policy, admin_id, _) = setup_policy();
        let user = policy.get_user(&admin_id).unwrap();
        let ctx = AuthContext::authenticated(admin_id, user.roles.clone());
        let result = ctx.require_permission(&policy, "workloads", Action::Create);
        assert!(result.is_ok());
    }

    #[test]
    fn auth_context_require_permission_denied() {
        let (policy, _, operator_id) = setup_policy();
        let user = policy.get_user(&operator_id).unwrap();
        let ctx = AuthContext::authenticated(operator_id, user.roles.clone());
        let result = ctx.require_permission(&policy, "nodes", Action::Delete);
        assert!(result.is_err());
    }

    // ===================
    // context_for_user Tests
    // ===================

    #[test]
    fn context_for_user_success() {
        let (policy, admin_id, _) = setup_policy();
        let ctx = context_for_user(&policy, &admin_id);
        assert!(ctx.is_ok());
        let ctx = ctx.ok().unwrap();
        assert!(ctx.is_authenticated());
        assert!(ctx.has_role("admin"));
    }

    #[test]
    fn context_for_user_not_found() {
        let policy = RbacPolicy::new();
        let fake_id = UserId::new();
        let result = context_for_user(&policy, &fake_id);
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::UserNotFound { .. })));
    }

    #[test]
    fn context_for_user_inactive() {
        let (mut policy, admin_id, _) = setup_policy();
        let user = policy.get_user_mut(&admin_id).unwrap();
        user.deactivate();
        let result = context_for_user(&policy, &admin_id);
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::PermissionDenied { .. })));
    }
}
