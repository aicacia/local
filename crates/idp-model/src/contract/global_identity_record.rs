#[cfg(feature = "std")]
use std::collections::BTreeMap;

#[cfg(not(feature = "std"))]
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalIdentityTable {
    Users,
    UserEmails,
    UserPhoneNumbers,
    UserPasswords,
    Applications,
    Clients,
    Keys,
    OAuth2AuthorizationCodes,
    OAuth2UserConsents,
    Devices,
    Roles,
    Permissions,
    RolePermissions,
    ApplicationUserRoles,
}

impl GlobalIdentityTable {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Users => "users",
            Self::UserEmails => "user_emails",
            Self::UserPhoneNumbers => "user_phone_numbers",
            Self::UserPasswords => "user_passwords",
            Self::Applications => "applications",
            Self::Clients => "clients",
            Self::Keys => "keys",
            Self::OAuth2AuthorizationCodes => "oauth2_authorization_codes",
            Self::OAuth2UserConsents => "oauth2_user_consents",
            Self::Devices => "devices",
            Self::Roles => "roles",
            Self::Permissions => "permissions",
            Self::RolePermissions => "role_permissions",
            Self::ApplicationUserRoles => "application_user_roles",
        }
    }

    #[must_use]
    pub const fn columns(&self) -> &'static [&'static str] {
        match self {
            Self::Users => &[
                "name",
                "given_name",
                "family_name",
                "middle_name",
                "nickname",
                "profile",
                "picture",
                "website",
                "sex",
                "birthdate",
                "zoneinfo",
                "locale",
                "created_at",
                "updated_at",
            ],
            Self::UserEmails => &[
                "user_id",
                "email",
                "verified",
                "primary",
                "created_at",
                "updated_at",
            ],
            Self::UserPhoneNumbers => &[
                "user_id",
                "phone_number",
                "verified",
                "primary",
                "created_at",
                "updated_at",
            ],
            Self::UserPasswords => &[
                "user_id",
                "active",
                "password_hash",
                "created_at",
                "updated_at",
            ],
            Self::Applications => &["name", "uri", "description", "created_at", "updated_at"],
            Self::Clients => &[
                "application_id",
                "client_id",
                "client_secret",
                "client_id_issued_at",
                "client_secret_expires_at",
                "client_name",
                "client_uri",
                "redirect_uris",
                "client_type",
                "profile",
                "token_endpoint_auth_method",
                "allowed_grant_types",
                "response_types",
                "allowed_scopes",
                "logo_uri",
                "contacts",
                "terms_of_service_uri",
                "policy_uri",
                "software_statement",
                "software_id",
                "software_version",
                "created_at",
                "updated_at",
            ],
            Self::Keys => &[
                "parent_id",
                "entity_type",
                "entity_id",
                "derivation_path",
                "derivation_index",
                "hardened",
                "name",
                "revoked_at",
                "expires_at",
                "created_at",
                "updated_at",
            ],
            Self::OAuth2AuthorizationCodes => &[
                "code",
                "client_id",
                "key_id",
                "redirect_uri",
                "scopes",
                "resource",
                "code_challenge",
                "code_challenge_method",
                "nonce",
                "expires_at",
                "consumed_at",
                "created_at",
                "updated_at",
            ],
            Self::OAuth2UserConsents => &[
                "user_id",
                "client_id",
                "redirect_uri",
                "scope",
                "created_at",
                "updated_at",
            ],
            Self::Devices => &[
                "name",
                "public_key",
                "address",
                "enrollment_code_hash",
                "enrollment_expires_at",
                "pairing_accepting_public_key",
                "state",
                "created_at",
                "updated_at",
                "revoked_at",
            ],
            Self::Roles | Self::Permissions => &[
                "application_id",
                "name",
                "description",
                "created_at",
                "updated_at",
            ],
            Self::RolePermissions => &["role_id", "permission_id", "created_at", "updated_at"],
            Self::ApplicationUserRoles => &[
                "user_id",
                "application_id",
                "role_id",
                "created_at",
                "updated_at",
            ],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalIdentityValue {
    Null,
    Integer(i64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GlobalIdentityRow {
    pub table: GlobalIdentityTable,
    pub id: i64,
    pub columns: BTreeMap<String, GlobalIdentityValue>,
}

impl GlobalIdentityRow {
    #[must_use]
    pub fn path(&self) -> String {
        format!("records/{}/{}.json", self.table.name(), self.id)
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.id > 0
            && self.columns.len() == self.table.columns().len()
            && self
                .table
                .columns()
                .iter()
                .all(|column| self.columns.contains_key(*column))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "std")]
    use std::collections::BTreeMap;

    #[cfg(not(feature = "std"))]
    use alloc::collections::BTreeMap;

    use super::{GlobalIdentityRow, GlobalIdentityTable, GlobalIdentityValue};

    #[test]
    fn requires_every_column_in_a_known_table() {
        let mut columns = BTreeMap::new();
        columns.insert(
            "name".to_owned(),
            GlobalIdentityValue::Text("device".to_owned()),
        );
        assert!(
            !GlobalIdentityRow {
                table: GlobalIdentityTable::Devices,
                id: 1,
                columns,
            }
            .is_valid()
        );
    }

    #[test]
    fn includes_active_device_pairing_state() {
        assert!(
            GlobalIdentityTable::Devices
                .columns()
                .contains(&"pairing_accepting_public_key")
        );
    }

    #[test]
    fn derives_stable_record_paths() {
        let row = GlobalIdentityRow {
            table: GlobalIdentityTable::Users,
            id: 12,
            columns: BTreeMap::new(),
        };
        assert_eq!(row.path(), "records/users/12.json");
    }
}
