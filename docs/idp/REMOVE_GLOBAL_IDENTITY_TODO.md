# Remove Global Identity Filesystem TODO

## Goal

Delete the filesystem global-identity snapshot replica. Native DB repositories,
synchronization, enrollment, and runtime composition replace it.

## Dependencies

- `DB_REPOSITORIES_TODO.md`
- `DB_SYNC_TRANSPORT_TODO.md`
- `DB_ENROLLMENT_TODO.md`

## Plan

- [x] Delete `GlobalIdentityRuntime`.
- [x] Delete `GlobalIdentityCache`.
- [x] Delete `GlobalIdentityRevisionWriter`.
- [x] Delete `GlobalIdentityReadGate` and its slot.
- [x] Delete global-identity manifest, record, and join snapshot contracts.
- [x] Delete global-identity filesystem vault setup and polling sync loop.
- [x] Delete bootstrap tunnel grants used only by that filesystem vault.
- [x] Delete stale global-identity runtime code from `storage-service`.
- [x] Remove old snapshot files from development setup and tests.
- [x] Remove obsolete LibSQL-only bootstrap paths and migrations.
- [x] Compose CLI and desktop runtimes from native DB repositories.

## Verification

Cargo checks are externally blocked. The following static searches and runtime
behavioral verification remain pending.

```sh
grep -RIn 'GlobalIdentityRuntime\|GlobalIdentityCache\|GlobalIdentityRevisionWriter\|GlobalIdentityReadGate' apps crates src
grep -RIn 'active-revision\|global-identity' apps crates src
cargo fmt --all --check
cargo test --workspace
```

The searches must return no runtime implementation references.
