//! Core types for authentication and authorization.
//!
//! This module defines the fundamental types:
//! - [`Permission`]: A resource-action pair for authorization
//! - [`Role`]: A named collection of permissions
//! - [`UserId`]: A validated user identifier
//! - [`User`]: A user with roles and API keys
//! - [`Scope`]: A scope for API keys and tokens

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use uuid::Uuid;

use crate::error::{Error, Result};

/// Actions that can be performed on resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Create a resource.
    Create,
    /// Read a resource.
    Read,
    /// Update a resource.
    Update,
    /// Delete a resource.
    Delete,
    /// List resources.
    List,
    /// Execute/run a resource.
    Execute,
    /// Administer a resource (full control).
    Admin,
}

impl Action {
    /// Returns all possible actions.
    #[must_use]
    pub fn all() -> &'static [Action] {
        &[
            Action::Create,
            Action::Read,
            Action::Update,
            Action::Delete,
            Action::List,
            Action::Execute,
            Action::Admin,
        ]
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Create => write!(f, "create"),
            Self::Read => write!(f, "read"),
            Self::Update => write!(f, "update"),
            Self::Delete => write!(f, "delete"),
            Self::List => write!(f, "list"),
            Self::Execute => write!(f, "execute"),
            Self::Admin => write!(f, "admin"),
        }
    }
}

impl std::str::FromStr for Action {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "create" => Ok(Self::Create),
            "read" => Ok(Self::Read),
            "update" => Ok(Self::Update),
            "delete" => Ok(Self::Delete),
            "list" => Ok(Self::List),
            "execute" => Ok(Self::Execute),
            "admin" => Ok(Self::Admin),
            _ => Err(Error::InvalidPermission {
                reason: format!("unknown action: {s}"),
            }),
        }
    }
}

/// A permission represents the ability to perform an action on a resource.
///
/// Resources are hierarchical, using colon-separated paths (e.g., "workloads:logs").
/// The wildcard "*" matches any resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Permission {
    /// The resource this permission applies to.
    pub resource: String,
    /// The action allowed on the resource.
    pub action: Action,
}

impl Permission {
    /// Creates a new permission.
    ///
    /// # Errors
    ///
    /// Returns an error if the resource is empty.
    pub fn new(resource: impl Into<String>, action: Action) -> Result<Self> {
        let resource = resource.into();
        if resource.is_empty() {
            return Err(Error::InvalidPermission {
                reason: "resource cannot be empty".to_string(),
            });
        }
        Ok(Self { resource, action })
    }

    /// Creates a wildcard permission that matches any resource for the given action.
    #[must_use]
    pub fn wildcard(action: Action) -> Self {
        Self {
            resource: "*".to_string(),
            action,
        }
    }

    /// Creates an admin permission for the resource (all actions).
    #[must_use]
    pub fn admin(resource: impl Into<String>) -> Self {
        Self {
            resource: resource.into(),
            action: Action::Admin,
        }
    }

    /// Checks if this permission matches the given resource and action.
    ///
    /// Admin permission on a resource implies all other actions.
    /// Wildcard resource "*" matches any resource.
    #[must_use]
    pub fn matches(&self, resource: &str, action: Action) -> bool {
        let resource_matches = self.resource == "*"
            || self.resource == resource
            || resource.starts_with(&format!("{}:", self.resource));

        let action_matches = self.action == Action::Admin || self.action == action;

        resource_matches && action_matches
    }

    /// Parses a permission from a string in the format "resource:action".
    ///
    /// # Errors
    ///
    /// Returns an error if the format is invalid.
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.rsplitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(Error::InvalidPermission {
                reason: format!("invalid format: expected 'resource:action', got '{s}'"),
            });
        }
        let action: Action = parts[0].parse()?;
        let resource = parts[1].to_string();
        Self::new(resource, action)
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.resource, self.action)
    }
}

/// A role is a named collection of permissions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    /// The unique name of the role.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// The permissions granted by this role.
    pub permissions: HashSet<Permission>,
}

impl Role {
    /// Creates a new role.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is empty or invalid.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Result<Self> {
        let name = name.into();
        Self::validate_name(&name)?;
        Ok(Self {
            name,
            description: description.into(),
            permissions: HashSet::new(),
        })
    }

    /// Adds a permission to this role.
    pub fn add_permission(&mut self, permission: Permission) {
        self.permissions.insert(permission);
    }

    /// Removes a permission from this role.
    pub fn remove_permission(&mut self, permission: &Permission) {
        self.permissions.remove(permission);
    }

    /// Checks if this role grants the given permission.
    #[must_use]
    pub fn has_permission(&self, resource: &str, action: Action) -> bool {
        self.permissions
            .iter()
            .any(|p| p.matches(resource, action))
    }

    /// Creates an admin role with full permissions.
    #[must_use]
    pub fn admin() -> Self {
        let mut role = Self {
            name: "admin".to_string(),
            description: "Full system administrator".to_string(),
            permissions: HashSet::new(),
        };
        role.add_permission(Permission::wildcard(Action::Admin));
        role
    }

    /// Creates a read-only role.
    #[must_use]
    pub fn readonly() -> Self {
        let mut role = Self {
            name: "readonly".to_string(),
            description: "Read-only access to all resources".to_string(),
            permissions: HashSet::new(),
        };
        role.add_permission(Permission::wildcard(Action::Read));
        role.add_permission(Permission::wildcard(Action::List));
        role
    }

    /// Creates an operator role for managing workloads.
    #[must_use]
    pub fn operator() -> Self {
        let mut role = Self {
            name: "operator".to_string(),
            description: "Manage workloads and view cluster status".to_string(),
            permissions: HashSet::new(),
        };
        // Can manage workloads
        for action in Action::all() {
            if let Ok(p) = Permission::new("workloads", *action) {
                role.add_permission(p);
            }
        }
        // Can read nodes and cluster
        if let Ok(p) = Permission::new("nodes", Action::Read) {
            role.add_permission(p);
        }
        if let Ok(p) = Permission::new("nodes", Action::List) {
            role.add_permission(p);
        }
        if let Ok(p) = Permission::new("cluster", Action::Read) {
            role.add_permission(p);
        }
        role
    }

    fn validate_name(name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(Error::InvalidRole {
                reason: "name cannot be empty".to_string(),
            });
        }
        if name.len() > 64 {
            return Err(Error::InvalidRole {
                reason: "name cannot exceed 64 characters".to_string(),
            });
        }
        let first = name.chars().next().ok_or_else(|| Error::InvalidRole {
            reason: "name cannot be empty".to_string(),
        })?;
        if !first.is_ascii_lowercase() {
            return Err(Error::InvalidRole {
                reason: "name must start with a lowercase letter".to_string(),
            });
        }
        for c in name.chars() {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' && c != '_' {
                return Err(Error::InvalidRole {
                    reason: format!("invalid character in name: '{c}'"),
                });
            }
        }
        Ok(())
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// A validated user identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UserId(String);

impl UserId {
    /// Maximum length of a user identifier.
    pub const MAX_LENGTH: usize = 128;

    /// Creates a new `UserId` from a UUID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Creates the anonymous user ID.
    /// This is a special constant ID used for unauthenticated contexts.
    #[must_use]
    pub fn anonymous() -> Self {
        Self("anonymous".to_string())
    }

    /// Creates a `UserId` from an existing string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid user ID.
    pub fn from_string(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        Self::validate(&id)?;
        Ok(Self(id))
    }

    /// Returns the ID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(id: &str) -> Result<()> {
        if id.is_empty() {
            return Err(Error::InvalidUserId {
                reason: "user id cannot be empty".to_string(),
            });
        }
        if id.len() > Self::MAX_LENGTH {
            return Err(Error::InvalidUserId {
                reason: format!(
                    "user id exceeds maximum length of {} characters",
                    Self::MAX_LENGTH
                ),
            });
        }
        Ok(())
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for UserId {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::from_string(value)
    }
}

impl From<UserId> for String {
    fn from(id: UserId) -> Self {
        id.0
    }
}

impl AsRef<str> for UserId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// A user in the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// Unique user identifier.
    pub id: UserId,
    /// Human-readable username.
    pub name: String,
    /// Email address.
    pub email: Option<String>,
    /// Roles assigned to this user.
    pub roles: Vec<String>,
    /// Whether the user is active.
    pub active: bool,
    /// When the user was created.
    pub created_at: DateTime<Utc>,
    /// When the user was last updated.
    pub updated_at: DateTime<Utc>,
}

impl User {
    /// Creates a new user.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is invalid.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(Error::InvalidUserId {
                reason: "name cannot be empty".to_string(),
            });
        }
        let now = Utc::now();
        Ok(Self {
            id: UserId::new(),
            name,
            email: None,
            roles: Vec::new(),
            active: true,
            created_at: now,
            updated_at: now,
        })
    }

    /// Creates a new user with a specific ID.
    #[must_use]
    pub fn with_id(id: UserId, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id,
            name: name.into(),
            email: None,
            roles: Vec::new(),
            active: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// Sets the email address.
    #[must_use]
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Adds a role to the user.
    pub fn add_role(&mut self, role: impl Into<String>) {
        let role = role.into();
        if !self.roles.contains(&role) {
            self.roles.push(role);
            self.updated_at = Utc::now();
        }
    }

    /// Removes a role from the user.
    pub fn remove_role(&mut self, role: &str) {
        if let Some(pos) = self.roles.iter().position(|r| r == role) {
            self.roles.remove(pos);
            self.updated_at = Utc::now();
        }
    }

    /// Checks if the user has a specific role.
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Deactivates the user.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.updated_at = Utc::now();
    }

    /// Activates the user.
    pub fn activate(&mut self) {
        self.active = true;
        self.updated_at = Utc::now();
    }
}

/// A scope defines the permissions granted to an API key or token.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Scope(String);

impl Scope {
    /// Creates a new scope.
    ///
    /// # Errors
    ///
    /// Returns an error if the scope is invalid.
    pub fn new(scope: impl Into<String>) -> Result<Self> {
        let scope = scope.into();
        Self::validate(&scope)?;
        Ok(Self(scope))
    }

    /// Returns the scope as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Checks if this scope grants access to the given resource and action.
    #[must_use]
    pub fn allows(&self, resource: &str, action: Action) -> bool {
        // Scope format: "resource:action" or "resource:*" or "*"
        if self.0 == "*" {
            return true;
        }

        let parts: Vec<&str> = self.0.splitn(2, ':').collect();
        if parts.len() != 2 {
            return false;
        }

        let scope_resource = parts[0];
        let scope_action = parts[1];

        let resource_matches = scope_resource == "*"
            || scope_resource == resource
            || resource.starts_with(&format!("{scope_resource}:"));

        let action_matches =
            scope_action == "*" || scope_action == action.to_string().as_str();

        resource_matches && action_matches
    }

    /// Parses multiple scopes from a space-separated string.
    ///
    /// # Errors
    ///
    /// Returns an error if any scope is invalid.
    pub fn parse_many(s: &str) -> Result<Vec<Self>> {
        s.split_whitespace().map(Self::new).collect()
    }

    fn validate(scope: &str) -> Result<()> {
        if scope.is_empty() {
            return Err(Error::InsufficientScope {
                required: "non-empty".to_string(),
                actual: "empty".to_string(),
            });
        }
        if scope.len() > 256 {
            return Err(Error::InsufficientScope {
                required: "max 256 chars".to_string(),
                actual: format!("{} chars", scope.len()),
            });
        }
        // Allow: alphanumeric, hyphen, underscore, colon, asterisk
        for c in scope.chars() {
            if !c.is_ascii_alphanumeric()
                && c != '-'
                && c != '_'
                && c != ':'
                && c != '*'
            {
                return Err(Error::InsufficientScope {
                    required: "valid characters".to_string(),
                    actual: format!("invalid character '{c}'"),
                });
            }
        }
        Ok(())
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for Scope {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Scope> for String {
    fn from(scope: Scope) -> Self {
        scope.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    // ===================
    // Action Tests
    // ===================

    #[test]
    fn action_display() {
        assert_eq!(Action::Create.to_string(), "create");
        assert_eq!(Action::Read.to_string(), "read");
        assert_eq!(Action::Update.to_string(), "update");
        assert_eq!(Action::Delete.to_string(), "delete");
        assert_eq!(Action::List.to_string(), "list");
        assert_eq!(Action::Execute.to_string(), "execute");
        assert_eq!(Action::Admin.to_string(), "admin");
    }

    #[test]
    fn action_parse() {
        assert_eq!("create".parse::<Action>().ok(), Some(Action::Create));
        assert_eq!("READ".parse::<Action>().ok(), Some(Action::Read));
        assert_eq!("Admin".parse::<Action>().ok(), Some(Action::Admin));
        assert!("invalid".parse::<Action>().is_err());
    }

    #[test]
    fn action_all() {
        let all = Action::all();
        assert_eq!(all.len(), 7);
        assert!(all.contains(&Action::Create));
        assert!(all.contains(&Action::Admin));
    }

    // ===================
    // Permission Tests
    // ===================

    #[test]
    fn permission_new_valid() {
        let p = Permission::new("workloads", Action::Create);
        assert!(p.is_ok());
        let p = p.ok();
        assert!(p.is_some());
        let p = p.unwrap();
        assert_eq!(p.resource, "workloads");
        assert_eq!(p.action, Action::Create);
    }

    #[test]
    fn permission_new_empty_resource() {
        let p = Permission::new("", Action::Create);
        assert!(p.is_err());
    }

    #[test]
    fn permission_wildcard() {
        let p = Permission::wildcard(Action::Read);
        assert_eq!(p.resource, "*");
        assert_eq!(p.action, Action::Read);
    }

    #[test]
    fn permission_admin() {
        let p = Permission::admin("workloads");
        assert_eq!(p.resource, "workloads");
        assert_eq!(p.action, Action::Admin);
    }

    #[test_case("workloads", Action::Create, true ; "exact match")]
    #[test_case("workloads", Action::Read, false ; "different action")]
    #[test_case("nodes", Action::Create, false ; "different resource")]
    #[test_case("workloads:logs", Action::Create, true ; "child resource")]
    fn permission_matches_exact(resource: &str, action: Action, expected: bool) {
        let p = Permission::new("workloads", Action::Create).ok();
        assert!(p.is_some());
        let p = p.unwrap();
        assert_eq!(p.matches(resource, action), expected);
    }

    #[test]
    fn permission_wildcard_matches_any() {
        let p = Permission::wildcard(Action::Read);
        assert!(p.matches("workloads", Action::Read));
        assert!(p.matches("nodes", Action::Read));
        assert!(p.matches("any:resource", Action::Read));
        assert!(!p.matches("workloads", Action::Create));
    }

    #[test]
    fn permission_admin_matches_all_actions() {
        let p = Permission::admin("workloads");
        assert!(p.matches("workloads", Action::Create));
        assert!(p.matches("workloads", Action::Read));
        assert!(p.matches("workloads", Action::Delete));
        assert!(p.matches("workloads", Action::Admin));
        assert!(!p.matches("nodes", Action::Read));
    }

    #[test]
    fn permission_display() {
        let p = Permission::new("workloads", Action::Create).ok();
        assert!(p.is_some());
        assert_eq!(p.unwrap().to_string(), "workloads:create");
    }

    #[test]
    fn permission_parse() {
        let p = Permission::parse("workloads:create").ok();
        assert!(p.is_some());
        let p = p.unwrap();
        assert_eq!(p.resource, "workloads");
        assert_eq!(p.action, Action::Create);
    }

    #[test]
    fn permission_parse_nested_resource() {
        let p = Permission::parse("workloads:logs:read");
        assert!(p.is_ok());
        let p = p.ok();
        assert!(p.is_some());
        let p = p.unwrap();
        assert_eq!(p.resource, "workloads:logs");
        assert_eq!(p.action, Action::Read);
    }

    #[test]
    fn permission_parse_invalid() {
        assert!(Permission::parse("invalid").is_err());
        assert!(Permission::parse(":read").is_err());
        assert!(Permission::parse("workloads:").is_err());
    }

    #[test]
    fn permission_serde_roundtrip() {
        let p = Permission::new("workloads", Action::Create).ok();
        assert!(p.is_some());
        let p = p.unwrap();
        let json = serde_json::to_string(&p);
        assert!(json.is_ok());
        let json = json.unwrap();
        let restored: Permission = serde_json::from_str(&json).ok().unwrap();
        assert_eq!(p, restored);
    }

    // ===================
    // Role Tests
    // ===================

    #[test]
    fn role_new_valid() {
        let r = Role::new("operator", "Manages workloads");
        assert!(r.is_ok());
        let r = r.ok();
        assert!(r.is_some());
        let r = r.unwrap();
        assert_eq!(r.name, "operator");
        assert_eq!(r.description, "Manages workloads");
        assert!(r.permissions.is_empty());
    }

    #[test]
    fn role_new_invalid_empty() {
        assert!(Role::new("", "desc").is_err());
    }

    #[test]
    fn role_new_invalid_starts_uppercase() {
        assert!(Role::new("Admin", "desc").is_err());
    }

    #[test]
    fn role_new_invalid_too_long() {
        let long = "a".repeat(65);
        assert!(Role::new(&long, "desc").is_err());
    }

    #[test]
    fn role_add_permission() {
        let r = Role::new("test", "Test role");
        assert!(r.is_ok());
        let mut r = r.ok().unwrap();
        let p = Permission::new("workloads", Action::Create);
        assert!(p.is_ok());
        r.add_permission(p.unwrap());
        assert_eq!(r.permissions.len(), 1);
        assert!(r.has_permission("workloads", Action::Create));
    }

    #[test]
    fn role_remove_permission() {
        let r = Role::new("test", "Test role");
        assert!(r.is_ok());
        let mut r = r.ok().unwrap();
        let p = Permission::new("workloads", Action::Create);
        assert!(p.is_ok());
        let p = p.ok().unwrap();
        r.add_permission(p.clone());
        r.remove_permission(&p);
        assert!(r.permissions.is_empty());
    }

    #[test]
    fn role_admin_has_all_permissions() {
        let admin = Role::admin();
        assert!(admin.has_permission("workloads", Action::Create));
        assert!(admin.has_permission("workloads", Action::Delete));
        assert!(admin.has_permission("nodes", Action::Admin));
        assert!(admin.has_permission("anything", Action::Execute));
    }

    #[test]
    fn role_readonly_has_only_read_list() {
        let readonly = Role::readonly();
        assert!(readonly.has_permission("workloads", Action::Read));
        assert!(readonly.has_permission("workloads", Action::List));
        assert!(!readonly.has_permission("workloads", Action::Create));
        assert!(!readonly.has_permission("workloads", Action::Delete));
    }

    #[test]
    fn role_operator() {
        let operator = Role::operator();
        assert!(operator.has_permission("workloads", Action::Create));
        assert!(operator.has_permission("workloads", Action::Delete));
        assert!(operator.has_permission("nodes", Action::Read));
        assert!(!operator.has_permission("nodes", Action::Delete));
    }

    #[test]
    fn role_display() {
        let r = Role::new("my-role", "desc");
        assert!(r.is_ok());
        assert_eq!(r.ok().unwrap().to_string(), "my-role");
    }

    // ===================
    // UserId Tests
    // ===================

    #[test]
    fn user_id_new() {
        let id = UserId::new();
        assert!(!id.as_str().is_empty());
    }

    #[test]
    fn user_id_from_string_valid() {
        let id = UserId::from_string("user-123");
        assert!(id.is_ok());
        assert_eq!(id.ok().unwrap().as_str(), "user-123");
    }

    #[test]
    fn user_id_from_string_empty() {
        assert!(UserId::from_string("").is_err());
    }

    #[test]
    fn user_id_from_string_too_long() {
        let long = "a".repeat(UserId::MAX_LENGTH + 1);
        assert!(UserId::from_string(&long).is_err());
    }

    #[test]
    fn user_id_display() {
        let id = UserId::from_string("test-user");
        assert!(id.is_ok());
        assert_eq!(id.ok().unwrap().to_string(), "test-user");
    }

    #[test]
    fn user_id_serde_roundtrip() {
        let id = UserId::from_string("test-user");
        assert!(id.is_ok());
        let id = id.ok().unwrap();
        let json = serde_json::to_string(&id);
        assert!(json.is_ok());
        let restored: UserId = serde_json::from_str(&json.unwrap()).ok().unwrap();
        assert_eq!(id, restored);
    }

    // ===================
    // User Tests
    // ===================

    #[test]
    fn user_new() {
        let user = User::new("alice");
        assert!(user.is_ok());
        let user = user.ok().unwrap();
        assert_eq!(user.name, "alice");
        assert!(user.active);
        assert!(user.roles.is_empty());
    }

    #[test]
    fn user_new_empty_name() {
        assert!(User::new("").is_err());
    }

    #[test]
    fn user_with_email() {
        let user = User::new("alice");
        assert!(user.is_ok());
        let user = user.ok().unwrap().with_email("alice@example.com");
        assert_eq!(user.email, Some("alice@example.com".to_string()));
    }

    #[test]
    fn user_add_role() {
        let user = User::new("alice");
        assert!(user.is_ok());
        let mut user = user.ok().unwrap();
        user.add_role("admin");
        assert!(user.has_role("admin"));
        assert!(!user.has_role("operator"));
    }

    #[test]
    fn user_add_role_idempotent() {
        let user = User::new("alice");
        assert!(user.is_ok());
        let mut user = user.ok().unwrap();
        user.add_role("admin");
        user.add_role("admin");
        assert_eq!(user.roles.len(), 1);
    }

    #[test]
    fn user_remove_role() {
        let user = User::new("alice");
        assert!(user.is_ok());
        let mut user = user.ok().unwrap();
        user.add_role("admin");
        user.remove_role("admin");
        assert!(!user.has_role("admin"));
    }

    #[test]
    fn user_deactivate_activate() {
        let user = User::new("alice");
        assert!(user.is_ok());
        let mut user = user.ok().unwrap();
        assert!(user.active);
        user.deactivate();
        assert!(!user.active);
        user.activate();
        assert!(user.active);
    }

    // ===================
    // Scope Tests
    // ===================

    #[test]
    fn scope_new_valid() {
        let s = Scope::new("workloads:read");
        assert!(s.is_ok());
        assert_eq!(s.ok().unwrap().as_str(), "workloads:read");
    }

    #[test]
    fn scope_new_wildcard() {
        let s = Scope::new("*");
        assert!(s.is_ok());
    }

    #[test]
    fn scope_new_empty() {
        assert!(Scope::new("").is_err());
    }

    #[test]
    fn scope_allows_exact() {
        let s = Scope::new("workloads:read");
        assert!(s.is_ok());
        let s = s.ok().unwrap();
        assert!(s.allows("workloads", Action::Read));
        assert!(!s.allows("workloads", Action::Create));
        assert!(!s.allows("nodes", Action::Read));
    }

    #[test]
    fn scope_allows_wildcard_action() {
        let s = Scope::new("workloads:*");
        assert!(s.is_ok());
        let s = s.ok().unwrap();
        assert!(s.allows("workloads", Action::Read));
        assert!(s.allows("workloads", Action::Create));
        assert!(s.allows("workloads", Action::Delete));
        assert!(!s.allows("nodes", Action::Read));
    }

    #[test]
    fn scope_allows_wildcard_all() {
        let s = Scope::new("*");
        assert!(s.is_ok());
        let s = s.ok().unwrap();
        assert!(s.allows("workloads", Action::Read));
        assert!(s.allows("nodes", Action::Delete));
        assert!(s.allows("anything", Action::Admin));
    }

    #[test]
    fn scope_allows_child_resource() {
        let s = Scope::new("workloads:read");
        assert!(s.is_ok());
        let s = s.ok().unwrap();
        assert!(s.allows("workloads:logs", Action::Read));
        assert!(!s.allows("workloads:logs", Action::Create));
    }

    #[test]
    fn scope_parse_many() {
        let scopes = Scope::parse_many("workloads:read nodes:list");
        assert!(scopes.is_ok());
        let scopes = scopes.ok().unwrap();
        assert_eq!(scopes.len(), 2);
        assert!(scopes[0].allows("workloads", Action::Read));
        assert!(scopes[1].allows("nodes", Action::List));
    }

    #[test]
    fn scope_display() {
        let s = Scope::new("workloads:read");
        assert!(s.is_ok());
        assert_eq!(s.ok().unwrap().to_string(), "workloads:read");
    }

    #[test]
    fn scope_serde_roundtrip() {
        let s = Scope::new("workloads:read");
        assert!(s.is_ok());
        let s = s.ok().unwrap();
        let json = serde_json::to_string(&s);
        assert!(json.is_ok());
        let restored: Scope = serde_json::from_str(&json.ok().unwrap()).ok().unwrap();
        assert_eq!(s, restored);
    }
}
