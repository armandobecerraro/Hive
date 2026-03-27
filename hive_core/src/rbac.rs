//! RBAC (Role-Based Access Control).
//! Define roles: admin, developer, viewer con permisos.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Role {
    Admin,
    Developer,
    Viewer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Permission {
    TriggerCycle,
    ApproveMerge,
    ViewStatus,
    Configure,
    ManageUsers,
    ManagePlugins,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub role: Role,
    pub api_key_hash: Option<String>,
}

pub struct RbacManager {
    users: HashMap<String, User>,
    role_permissions: HashMap<Role, Vec<Permission>>,
}

impl RbacManager {
    pub fn new() -> Self {
        let mut role_perms = HashMap::new();
        role_perms.insert(
            Role::Admin,
            vec![
                Permission::TriggerCycle,
                Permission::ApproveMerge,
                Permission::ViewStatus,
                Permission::Configure,
                Permission::ManageUsers,
                Permission::ManagePlugins,
            ],
        );
        role_perms.insert(
            Role::Developer,
            vec![
                Permission::TriggerCycle,
                Permission::ApproveMerge,
                Permission::ViewStatus,
            ],
        );
        role_perms.insert(Role::Viewer, vec![Permission::ViewStatus]);

        Self {
            users: HashMap::new(),
            role_permissions: role_perms,
        }
    }

    pub fn add_user(&mut self, username: &str, role: Role) {
        self.users.insert(
            username.into(),
            User {
                username: username.into(),
                role,
                api_key_hash: None,
            },
        );
    }

    pub fn check_permission(&self, username: &str, permission: &Permission) -> bool {
        if let Some(user) = self.users.get(username) {
            if let Some(perms) = self.role_permissions.get(&user.role) {
                return perms.contains(permission);
            }
        }
        false
    }

    pub fn get_user(&self, username: &str) -> Option<&User> {
        self.users.get(username)
    }

    pub fn list_users(&self) -> Vec<&User> {
        self.users.values().collect()
    }

    pub fn remove_user(&mut self, username: &str) -> bool {
        self.users.remove(username).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_has_all_permissions() {
        let mut rbac = RbacManager::new();
        rbac.add_user("admin", Role::Admin);
        assert!(rbac.check_permission("admin", &Permission::ManageUsers));
        assert!(rbac.check_permission("admin", &Permission::TriggerCycle));
    }

    #[test]
    fn viewer_cannot_trigger() {
        let mut rbac = RbacManager::new();
        rbac.add_user("viewer", Role::Viewer);
        assert!(!rbac.check_permission("viewer", &Permission::TriggerCycle));
        assert!(rbac.check_permission("viewer", &Permission::ViewStatus));
    }

    #[test]
    fn unknown_user_no_permissions() {
        let rbac = RbacManager::new();
        assert!(!rbac.check_permission("unknown", &Permission::ViewStatus));
    }
}
