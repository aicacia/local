# Remove Global Identity Filesystem TODO

## Goal

Delete the filesystem snapshot replica after DB-backed repositories,
synchronization, and enrollment are verified.

## Dependencies

- `DB_REPOSITORIES_TODO.md`
- `DB_SYNC_TRANSPORT_TODO.md`
- `DB_ENROLLMENT_TODO.md`

## Plan

- [ ] Delete `GlobalIdentityRuntime`.
- [ ] Delete `GlobalIdentityCache`.
- [ ] Delete `GlobalIdentityRevisionWriter`.
- [ ] Delete `GlobalIdentityReadGate` and its slot.
- [ ] Delete global-identity manifest, record, and join snapshot contracts.
- [ ] Delete global-identity filesystem vault setup and polling sync loop.
- [ ] Delete bootstrap tunnel grants used only by that filesystem vault.
- [ ] Delete stale global-identity runtime code from `storage-service`.
- [ ] Remove old snapshot files from development setup and tests.
- [ ] Remove obsolete LibSQL-only bootstrap paths.

## Verification

```sh
grep -RIn 'GlobalIdentityRuntime\|GlobalIdentityCache\|GlobalIdentityRevisionWriter\|GlobalIdentityReadGate' apps crates src
grep -RIn 'active-revision\|global-identity' apps crates src
cargo fmt --all --check
cargo test --workspace
```

The searches must return no runtime implementation references.
