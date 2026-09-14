# Storage Architecture Refactor (2026-09-12)

Goal: make the syncable filesystem a library-first local filesystem that works without a transport, with optional replication attached above it.

## Checklist

- [x] Add a transport-free local filesystem entry point.
- [x] Add rename semantics and reserved internal-path validation.
- [ ] Separate local filesystem state from replication session ownership.
- [x] Extract transport task lifecycle and message dispatch into an internal `SyncSession`.
- [ ] Make persistence recovery explicit for metadata, content, and outbound changes.
- [ ] Keep transport adapters independent of filesystem and namespace policy.
- [ ] Reduce storage-service to namespace, residency, and composition concerns.
- [x] Unify storage-server session execution for generic and scoped services.
- [ ] Migrate application consumers to the library-first construction path.
- [x] Document local-only and optional-sync usage.

## Verification

- [x] `cargo test -p file-system --all-targets`
- [x] `cargo check -p file-system --no-default-features`
- [x] `cargo clippy -p file-system --all-targets -- -D warnings`
- [x] `cargo test -p storage-model -p storage-service -p storage-server -p idp-server --all-targets`
- [x] `cargo hack test -p file-system --feature-powerset --all-targets`
- [x] `cargo test -p iroh-chain-file-system --all-targets`
- [x] `cargo check --workspace`
- [ ] `cargo hack test --feature-powerset --workspace --all-targets` (blocked by `libsql-ffi` build artifact creation failure)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` (blocked by existing `idp-server` `cloned_ref_to_slice_refs` test lints)

## Scope

The first redesign targets a Rust-native API. OS mounts, database-backed filesystem storage, and a binary WebSocket protocol remain outside this checklist until the ownership boundaries stabilize.

## User Notes (2026-09-13)

seperating the local version is a really bad idea, and some of this works should be changed to not do this, it breaks the boundaries. we should not have to create a new filesystem saying its local, it is local by default, transport is optional and that syncs.
