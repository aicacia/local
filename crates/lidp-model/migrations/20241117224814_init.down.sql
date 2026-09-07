DROP INDEX IF EXISTS "idx_application_user_roles_role_id";
DROP INDEX IF EXISTS "idx_application_user_roles_application_id";
DROP INDEX IF EXISTS "idx_application_user_roles_user_id";
DROP TABLE IF EXISTS "application_user_roles";

DROP INDEX IF EXISTS "idx_role_permissions_permission_id";
DROP INDEX IF EXISTS "idx_role_permissions_role_id";
DROP TABLE IF EXISTS "role_permissions";

DROP INDEX IF EXISTS "idx_permissions_application_name";
DROP INDEX IF EXISTS "idx_permissions_application_id";
DROP TABLE IF EXISTS "permissions";

DROP INDEX IF EXISTS "idx_roles_application_name";
DROP INDEX IF EXISTS "idx_roles_application_id";
DROP TABLE IF EXISTS "roles";

DROP INDEX IF EXISTS "idx_oauth2_user_consents_user_client";
DROP TABLE IF EXISTS "oauth2_user_consents";

DROP INDEX IF EXISTS "idx_oauth2_authorization_codes_expires";
DROP INDEX IF EXISTS "idx_oauth2_authorization_codes_key_id";
DROP INDEX IF EXISTS "idx_oauth2_authorization_codes_client";
DROP TABLE IF EXISTS "oauth2_authorization_codes";

DROP INDEX IF EXISTS "idx_keys_entity_type_entity_id";
DROP INDEX IF EXISTS "idx_keys_parent_id";
DROP INDEX IF EXISTS "idx_keys_derivation_path";
DROP TABLE IF EXISTS "keys";

DROP INDEX IF EXISTS "idx_clients_application_id";
DROP INDEX IF EXISTS "idx_clients_client_name";
DROP TABLE IF EXISTS "clients";

DROP INDEX IF EXISTS "idx_applications_uri";
DROP INDEX IF EXISTS "idx_applications_name";
DROP TABLE IF EXISTS "applications";

DROP INDEX IF EXISTS "idx_user_passwords_user_id";
DROP TABLE IF EXISTS "user_passwords";

DROP INDEX IF EXISTS "idx_user_phone_numbers_primary";
DROP INDEX IF EXISTS "idx_user_phone_numbers_user_id";
DROP TABLE IF EXISTS "user_phone_numbers";

DROP INDEX IF EXISTS "idx_user_emails_primary";
DROP INDEX IF EXISTS "idx_user_emails_user_id";
DROP TABLE IF EXISTS "user_emails";

DROP INDEX IF EXISTS "idx_device_pairing_invitations_enrollment";
DROP TABLE IF EXISTS "device_pairing_invitations";

DROP INDEX IF EXISTS "idx_user_devices_user_state";
DROP INDEX IF EXISTS "idx_user_devices_user_public_key";
DROP TABLE IF EXISTS "user_devices";

DROP INDEX IF EXISTS "idx_users_name";
DROP TABLE IF EXISTS "users";
