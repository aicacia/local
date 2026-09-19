# IdP DB Repositories TODO

## Goal

Replace `LibSql*Repo` implementations with concrete repositories backed by
the native syncable `db` engine.

## Dependencies

- `DB_SCHEMA_TODO.md`
- `DB_CONFLICT_POLICY_TODO.md`

## Independent scope

Each repository family can be implemented independently once the shared schema
helper exists.

- [ ] Add DB-backed application and client repositories.
- [ ] Add DB-backed user, credential, and key repositories.
- [ ] Add DB-backed OAuth authorization-code and consent repositories.
- [ ] Add DB-backed device, role, and permission repositories.
- [ ] Preserve existing service interfaces where they already model the domain.
- [ ] Enforce conflict policy at every security-sensitive read.
- [ ] Replace CLI and desktop service construction with DB repositories.

## Verification

- [ ] Bootstrap writes the system baseline through DB repositories.
- [ ] OAuth authorize, token, refresh, exchange, and userinfo work.
- [ ] Management CRUD works.
- [ ] A conflicted security row is denied by its repository.
