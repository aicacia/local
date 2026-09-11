ALTER TABLE `devices`
    DROP COLUMN `pairing_accepting_public_key`;

CREATE TABLE `device_pairing_invitations` (
    `id` INTEGER PRIMARY KEY,
    `initiating_public_key` TEXT NOT NULL,
    `secret_hash` BLOB NOT NULL UNIQUE,
    `expires_at` INTEGER NOT NULL,
    `redeemed_at` INTEGER,
    `enrollment_device_id` INTEGER UNIQUE
        REFERENCES `devices`(`id`) ON DELETE SET NULL,
    `created_at` INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE INDEX `idx_device_pairing_invitations_enrollment`
    ON `device_pairing_invitations`(`enrollment_device_id`);
