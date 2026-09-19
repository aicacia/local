# IdP UUID Identity TODO

## Goal

Make the IdP domain, service interfaces, and wire contracts use the UUID
identities required by the replicated native DB schema.

## Why first

`DB_SCHEMA_TODO.md` correctly requires UUID primary keys, but the current IdP
models and repository traits still use `i64`. Do not add a UUID-to-`i64`
translation table: it creates another replicated identity authority and breaks
foreign-key identity across replicas.

## Plan

- [x] Add shared UUID ID types in IdP models.
- [x] Change model IDs and foreign keys for users, applications, clients,
      devices, roles, permissions, keys, OAuth artifacts, and grants to UUID.
- [x] Change repository and default service method signatures to UUID.
      Legacy filesystem and LibSQL repositories are removed.
- [x] Change bootstrap, management, tunnel, storage-namespace, and OAuth
      call sites to UUID.
- [x] Remove auto-increment assumptions and random `i64` ID generators.
- [x] Update serialization contracts and OpenAPI schemas.
- [x] Delete the global-identity snapshot row contract with its `i64` ID.

## Verification

Cargo checks are externally blocked. Runtime behavioral verification remains
pending.

- [ ] `grep -RIn 'pub id: i64\|application_id: i64\|user_id: i64\|device_id: i64\|role_id: i64\|permission_id: i64' crates apps` returns no persisted IdP identity fields.
- [ ] Bootstrap creates UUID-linked IdP records.
- [ ] OAuth, management, storage, and tunnel authorization retain the same ID
      across serialization and native DB reads.
- [ ] `cargo test -p idp-model`
- [ ] `cargo test -p idp-service`
