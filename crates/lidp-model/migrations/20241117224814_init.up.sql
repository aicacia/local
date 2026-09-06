CREATE TABLE users (
    `id` INTEGER PRIMARY KEY,

    `name` TEXT NOT NULL,

    `given_name` TEXT,
    `family_name` TEXT,
    `middle_name` TEXT,
    `nickname` TEXT,

    `profile` TEXT,
    `picture` TEXT,
    `website` TEXT,

    `sex` INTEGER,
    `birthdate` TEXT,

    `zoneinfo` TEXT,
    `locale` TEXT,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX `idx_users_name` ON `users`(`name`);

CREATE TABLE `user_emails` (
    `id` INTEGER PRIMARY KEY,

    `user_id` INTEGER NOT NULL
        REFERENCES `users`(`id`) ON DELETE CASCADE,

    `email` TEXT NOT NULL,

    `verified` INTEGER NOT NULL DEFAULT 0,
    `primary` INTEGER NOT NULL DEFAULT 0,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch()),

    UNIQUE(`user_id`, `email`)
) STRICT;

CREATE INDEX `idx_user_emails_user_id`
    ON `user_emails`(`user_id`);

CREATE UNIQUE INDEX `idx_user_emails_primary`
    ON `user_emails`(`user_id`)
    WHERE `primary` = 1;

CREATE TABLE user_devices (
    `id` INTEGER PRIMARY KEY,

    `user_id` INTEGER NOT NULL
        REFERENCES `users`(`id`) ON DELETE CASCADE,

    `name` TEXT NOT NULL,
    `public_key` TEXT NOT NULL,
    `address` TEXT NOT NULL,
    `enrollment_code_hash` BLOB,
    `enrollment_expires_at` INTEGER,
    `state` INTEGER NOT NULL CHECK (`state` IN (0, 1, 2)),

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `revoked_at` INTEGER
) STRICT;

CREATE UNIQUE INDEX `idx_user_devices_user_public_key`
    ON `user_devices`(`user_id`, `public_key`);

CREATE INDEX `idx_user_devices_user_state`
    ON `user_devices`(`user_id`, `state`);

CREATE TABLE user_phone_numbers (
    `id` INTEGER PRIMARY KEY,

    `user_id` INTEGER NOT NULL
        REFERENCES `users`(`id`) ON DELETE CASCADE,

    `phone_number` TEXT NOT NULL,

    `verified` INTEGER NOT NULL DEFAULT 0,
    `primary` INTEGER NOT NULL DEFAULT 0,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch()),

    UNIQUE(`user_id`, `phone_number`)
) STRICT;

CREATE INDEX `idx_user_phone_numbers_user_id`
    ON `user_phone_numbers`(`user_id`);

CREATE UNIQUE INDEX `idx_user_phone_numbers_primary`
    ON `user_phone_numbers`(`user_id`)
    WHERE `primary` = 1;

CREATE TABLE `user_passwords` (
    `id` INTEGER PRIMARY KEY,

    `user_id` INTEGER NOT NULL
        REFERENCES `users`(`id`) ON DELETE CASCADE,

    `active` INTEGER NOT NULL DEFAULT 1,
    `password_hash` TEXT NOT NULL,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX `idx_user_passwords_user_id`
    ON `user_passwords`(`user_id`);

CREATE TABLE `applications` (
    `id` INTEGER PRIMARY KEY,

    `name` TEXT NOT NULL,
    `uri` TEXT NOT NULL UNIQUE,
    `description` TEXT,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX `idx_applications_name`
    ON `applications`(`name`);

CREATE UNIQUE INDEX `idx_applications_uri`
    ON `applications`(`uri`);

CREATE TABLE `clients` (
    `id` INTEGER PRIMARY KEY,

    `application_id` INTEGER NOT NULL
        REFERENCES `applications`(`id`) ON DELETE CASCADE,

    `client_id` TEXT NOT NULL UNIQUE,
    `client_secret` TEXT NOT NULL,

    `client_id_issued_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `client_secret_expires_at` INTEGER,

    `client_name` TEXT NOT NULL,
    `client_uri` TEXT NOT NULL,

    `redirect_uris` TEXT NOT NULL,

    `client_type` INTEGER NOT NULL,
    `profile` INTEGER NOT NULL,

    `token_endpoint_auth_method` INTEGER NOT NULL,

    `allowed_grant_types` TEXT NOT NULL,
    `response_types` TEXT NOT NULL,

    `allowed_scopes` TEXT NOT NULL,

    `logo_uri` TEXT,

    `contacts` TEXT NOT NULL,

    `terms_of_service_uri` TEXT,
    `policy_uri` TEXT,

    `software_statement` TEXT,
    `software_id` TEXT,
    `software_version` TEXT,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX `idx_clients_application_id`
    ON `clients`(`application_id`);

CREATE INDEX `idx_clients_client_name`
    ON `clients`(`client_name`);

CREATE TABLE `keys` (
    `id` INTEGER PRIMARY KEY,

    `parent_id` INTEGER
        REFERENCES `keys`(`id`) ON DELETE CASCADE,

    `entity_type` INTEGER NOT NULL,
    `entity_id` INTEGER NOT NULL,

    `derivation_path` TEXT UNIQUE,
    `hardened` INTEGER NOT NULL,
    `name` TEXT NOT NULL,

    `revoked_at` INTEGER,
    `expires_at` INTEGER,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX `idx_keys_parent_id` ON `keys`(`parent_id`);
CREATE UNIQUE INDEX `idx_keys_entity_type_entity_id`
    ON `keys`(`entity_type`, `entity_id`);

CREATE TABLE `oauth2_authorization_codes` (
    `id` INTEGER PRIMARY KEY,

    `code` TEXT NOT NULL UNIQUE,

    `client_id` TEXT NOT NULL
        REFERENCES `clients`(`client_id`) ON DELETE CASCADE,

    `key_id` INTEGER NOT NULL
        REFERENCES `keys`(`id`) ON DELETE CASCADE,

    `redirect_uri` TEXT NOT NULL,

    `scopes` TEXT NOT NULL,

    `resource` TEXT,

    `code_challenge` TEXT,
    `code_challenge_method` INTEGER,

    `nonce` TEXT,

    `expires_at` INTEGER NOT NULL,
    `consumed_at` INTEGER,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX `idx_oauth2_authorization_codes_client`
    ON `oauth2_authorization_codes`(`client_id`);

CREATE INDEX `idx_oauth2_authorization_codes_key_id`
    ON `oauth2_authorization_codes`(`key_id`);

CREATE INDEX `idx_oauth2_authorization_codes_expires`
    ON `oauth2_authorization_codes`(`expires_at`);

CREATE TABLE `oauth2_user_consents` (
    `id` INTEGER PRIMARY KEY,

    `user_id` INTEGER NOT NULL
        REFERENCES `users`(`id`) ON DELETE CASCADE,

    `client_id` TEXT NOT NULL
        REFERENCES `clients`(`client_id`) ON DELETE CASCADE,

    `redirect_uri` TEXT NOT NULL,

    `scope` TEXT NOT NULL,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch()),

    UNIQUE(`user_id`, `client_id`, `redirect_uri`, `scope`)
) STRICT;

CREATE INDEX `idx_oauth2_user_consents_user_client`
    ON `oauth2_user_consents`(`user_id`, `client_id`);

CREATE TABLE roles (
    `id` INTEGER PRIMARY KEY,

    `application_id` INTEGER NOT NULL,

    `name` TEXT NOT NULL UNIQUE,
    `description` TEXT,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX `idx_roles_application_id`
    ON `roles`(`application_id`);

CREATE UNIQUE INDEX `idx_roles_application_name`
    ON `roles`(`application_id`, `name`);

CREATE TABLE permissions (
    `id` INTEGER PRIMARY KEY,

    `application_id` INTEGER NOT NULL,

    `name` TEXT NOT NULL UNIQUE,
    `description` TEXT,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX `idx_permissions_application_id`
    ON `roles`(`application_id`);

CREATE UNIQUE INDEX `idx_permissions_application_name`
    ON `roles`(`application_id`, `name`);

CREATE TABLE role_permissions (
    `id` INTEGER PRIMARY KEY,

    `role_id` INTEGER NOT NULL
        REFERENCES `roles`(`id`) ON DELETE CASCADE,

    `permission_id` INTEGER NOT NULL
        REFERENCES `permissions`(`id`) ON DELETE CASCADE,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch()),

    UNIQUE(`role_id`, `permission_id`)
) STRICT;

CREATE INDEX `idx_role_permissions_role_id`
    ON `role_permissions`(`role_id`);

CREATE INDEX `idx_role_permissions_permission_id`
    ON `role_permissions`(`permission_id`);

CREATE TABLE application_user_roles (
    `id` INTEGER PRIMARY KEY,

    `user_id` INTEGER NOT NULL
        REFERENCES `users`(`id`) ON DELETE CASCADE,

    `application_id` INTEGER NOT NULL,

    `role_id` INTEGER NOT NULL
        REFERENCES `roles`(`id`) ON DELETE CASCADE,

    `created_at` INTEGER NOT NULL DEFAULT (unixepoch()),
    `updated_at` INTEGER NOT NULL DEFAULT (unixepoch()),

    UNIQUE(`user_id`, `application_id`, `role_id`)
) STRICT;

CREATE INDEX `idx_application_user_roles_user_id`
    ON `application_user_roles`(`user_id`);

CREATE INDEX `idx_application_user_roles_application_id`
    ON `application_user_roles`(`application_id`);

CREATE INDEX `idx_application_user_roles_role_id`
    ON `application_user_roles`(`role_id`);
