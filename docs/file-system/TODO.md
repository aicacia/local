# File System Follow-up TODO

The web-access-server roadmap now lives in [`../storage/TODO.md`](../storage/TODO.md), next to [ADR-001](../storage/ARD01.md).

## Scope

Keep this document limited to core filesystem work. The completed local, metadata-sync, content-transfer, residency, and typed iroh transport phases remain the baseline.

## 1. Migrate filesystem callers

- Migrate intentionally broken callers to `FileSystem<PeerId>`, `Transport<PeerId>`, and `MetadataSync`.
- Remove obsolete compatibility code rather than restoring deleted generic storage, codec, or raw-byte transport APIs.
- Remove empty `in-memory`, `native`, and `sync` feature flags after no manifest selects them.

Validation per migrated crate:

```sh
cargo test -p <crate>
cargo check -p <crate>
```

Then:

```sh
cargo hack test -p file-system --feature-powerset --all-targets
```

## Deferred core work

Start only with a concrete consumer and end-to-end tests:

1. deterministic merge handling for supported content types
2. chunked content transfer
3. coordinated tombstone and unreferenced-content garbage collection
4. FUSE in a binary outside `crates/file-system`

## Final validation

Run after caller migration:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo hack test --feature-powerset --all-targets
cargo crap --all-targets
cargo test --workspace
```
