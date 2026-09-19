# IdP DB Repositories TODO

## Goal

Use concrete repositories backed by the native syncable `db` engine. The
legacy LibSQL adapters and migrations are removed.

## Dependencies

- `DB_SCHEMA_TODO.md`
- `DB_CONFLICT_POLICY_TODO.md`
- `UUID_IDENTITY_TODO.md`

## Independent scope

Each repository family can be implemented independently once the shared schema
helper exists.

- [x] Add DB-backed application and client repositories.
- [x] Add DB-backed key repository with UUID identity and conflict-gated reads.
- [x] Add DB-backed user and credential repositories.
- [x] Add DB-backed OAuth authorization-code and consent repositories.
- [x] Add DB-backed device, role, and permission repositories.
- [x] Preserve existing service interfaces where they already model the domain.
- [x] Enforce conflict policy at every security-sensitive read.
- [x] Remove LibSQL repository adapters and migrations.
- [x] Replace CLI and desktop service construction with DB repositories.

## Verification

Cargo checks are externally blocked. Runtime behavioral verification remains
pending.

- [ ] Bootstrap writes the system baseline through DB repositories.
- [ ] OAuth authorize, token, refresh, exchange, and userinfo work.
- [ ] Management CRUD works.
- [ ] A conflicted security row is denied by its repository.
