DROP INDEX IF EXISTS `idx_device_pairing_invitations_enrollment`;
DROP TABLE IF EXISTS `device_pairing_invitations`;

ALTER TABLE `devices`
    ADD COLUMN `pairing_accepting_public_key` TEXT;
