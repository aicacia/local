CREATE TABLE `global_identity_cache` (
    `id` INTEGER PRIMARY KEY CHECK (`id` = 1),
    `revision` TEXT NOT NULL,
    `applied_at` INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;
