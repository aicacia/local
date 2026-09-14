# ADR-002: Offline-First, Eventually Consistent Distributed File System

## Status

Accepted — supersedes ADR-001 (automerge/per-folder-doc design)

## Context

We need a Rust library for local file access that:

- Presents a native-OS-filesystem-like API (open, read, write, stream, list) and is mountable as a real filesystem on Linux (FUSE)
- Works fully offline and online
- Auto-syncs across a network of peers over our iroh-based chain, using eventual consistency with no central coordinator
- Lets each device exclude applications or choose full/passthrough storage at namespace, folder, and file granularity
- Keeps the transport (iroh) swappable behind a trait
- Resolves all conflicts deterministically — any two replicas that have seen the same updates must converge to the same result, regardless of arrival order

## Decision

### 1. Layered architecture

```
┌─────────────────────────────┐
│   Public API (FS-like)       │  open, read, write, stream, list
├─────────────────────────────┤
│   FUSE mount (Linux)         │  translates syscalls to the API above
├─────────────────────────────┤
│   Sync Engine                 │  owns the keyspace, merge rules, sync protocol
├─────────────────────────────┤
│   Metadata: flat key-value store │  path = key; per-key LWW-register
├─────────────────────────────┤
│   Storage Layer (native filesystem) │  content-addressed blobs
├─────────────────────────────┤
│   Transport Trait (minimal)   │  send / broadcast / subscribe, no topics
├─────────────────────────────┤
│   iroh (default impl)         │  swappable
└─────────────────────────────┘
```

### 2. Transport trait — minimal, dumb pipe

The transport's only job is moving bytes between peers. It knows nothing about files, sync, or merge logic — and it doesn't expose topics/channels either, since most transports won't have that concept. Any topic/routing needed internally (e.g. iroh gossip topics) stays inside the impl.

```rust
trait Transport {
    type PeerId;

    async fn send(&self, peer: Self::PeerId, data: Bytes);
    async fn broadcast(&self, data: Bytes);
    async fn subscribe(&self) -> Stream<(Self::PeerId, Bytes)>;
}
```

iroh is the default implementation. Swapping to a different transport later means implementing these three methods — nothing else in the library changes.

### 3. Metadata layer — flat key-value store, S3-style

- Storage follows the S3 pattern: a flat object store. Full paths act as keys. There is no real directory tree — "folders" are a virtual view derived from key prefixes.
- Every file has a stable identity independent of its key: a **UUID v7**. The key (path) is only how the file is accessed via FUSE — renaming changes the key, never the id. UUID v7's time-ordered structure also makes it usable directly as a stable FUSE inode number, considered a non-issue for the 99.9% of use cases (extremely low collision probability, monotonically-ish increasing so no reuse-after-remount concerns).
- No Merkle Search Tree and no CRDT library (e.g. automerge) are used. Every key's existence/pointer/tombstone state is resolved by a per-key **LWW-register**:
  - Each key's metadata entry holds `{ file_id (uuid v7), pointer/hash, size, providers, local: bool, merge_strategy, timestamp, replica_id, tombstone }`.
  - Merge is a pure, deterministic function: compare `timestamp` (HLC-based, not wall clock); on equal timestamps, tie-break on a stable `replica_id`. This is commutative, associative, and idempotent, so replicas converge regardless of message order.
  - Deletes are tombstones — the same LWW treatment applies to the `tombstone` field as to any other field.
- Rename is a first-class operation (not delete+create).
- Empty "folders" require an explicit marker object (there's no folder concept in a flat store), written synchronously so `mkdir`+`ls` behaves as expected on a single device. Deleting a virtual folder is a multi-key operation with no cross-key atomicity.

### 4. Sync engine — the library's responsibility

- Owns the keyspace and drives the sync protocol over the `Transport` trait (transport just shuttles opaque bytes).
- One conflict-resolution layer, not two:
  - **Key existence / pointer / rename / delete** conflicts are resolved entirely by the deterministic per-key LWW-register described above.
  - **File content (blob) conflicts** — two peers editing the same file's content while offline — are resolved according to the file's **type**, not a generic config setting. `merge_strategy` is a property of the file type:
    - Most file types default to **LWW** (last write wins on the blob as a whole).
    - Some file types use custom merge logic via a **plugin** (e.g. a file that is itself an automerge document is merged with automerge's own merge, not LWW, so edits aren't silently dropped).
    - Plugins are **compiled into the codebase and version-controlled**, not installed independently per device — every replica on a given build has an identical plugin set, which removes cross-device plugin-mismatch divergence (cross-_build_-version skew remains a separate, open concern).
    - Dispatch is a simple enum keyed on file extension/mime type — no trait objects or dynamic registry needed:
      ```rust
      enum MergeStrategy {
          Lww,
          AutomergeDoc,
          // new variants added here as new file types need custom merge behavior
      }
      ```
    - A plugin's merge output becomes a new value written back into the key-register under a fresh timestamp, so it wins the next LWW comparison — plugin merges feed the key layer rather than bypassing it.
    - **LWW is always the fallback**: if a replica encounters a file type whose plugin it doesn't have, it falls back to LWW deterministically rather than failing or blocking. In practice this fallback path is expected to be dormant — version negotiation for older builds lacking a plugin is out of scope, since all plugins discussed here ship in the initial release.
- **Determinism is a hard requirement** across the whole engine: HLC timestamp generation, tie-break ordering, and plugin merge functions must all be pure and reproducible. In particular, plugin code must avoid non-deterministic iteration order (e.g. Rust's `HashMap` — use `BTreeMap` or explicit sorting) and any dependence on wall-clock reads or platform-specific arithmetic. HLC timestamps are generated using an existing, audited Rust crate rather than a hand-rolled implementation.
- Tombstones are purged by a **configurable timed GC job**, once a day by default, rather than a precise per-tombstone watermark.
- The engine is also responsible for fetching blob content on demand (see passthrough, below), separate from metadata/key sync.
- The system is **local-first and reactive**: nodes act on changes as they arrive and never block waiting on a request/response round-trip to the network.

### 5. Blob storage

- Files are content-addressed (hashed) and chunked, enabling streaming reads and integrity checks.
- Full files are stored as native filesystem blobs under `.blobs/<hash>`.
- Key-register entries never contain blob bytes inline for full files — only metadata pointing at them.

### 6. Device-local storage residency

Storage residency is device-local policy; it is not synchronized metadata.

- A device may exclude an entire Application. Exclusion applies to every Storage Namespace for that application and prevents local metadata and blob storage from being created.
- An included application defaults to **Passthrough**.
- A device may set **Full** or **Passthrough** for a namespace, folder, or file. The longest matching path rule wins.
- The Global Identity Namespace is always Full and cannot be excluded.
- Full residency stores metadata and blobs locally. Passthrough stores metadata but no blob bytes, and is not cached locally by the library (though the OS page cache may still cache recently-read pages — see §9).
- Changing from Passthrough to Full fetches and verifies all selected blobs before marking the device as a provider. Changing from Full to Passthrough removes local references and garbage-collects only blobs unreferenced by other locally-full entries.

### 7. Passthrough reads and writes

- Every passthrough read goes to an online Full peer and streams the blob by hash.
- A passthrough write requires an online Full peer. The writer uploads the blob to that peer; the peer verifies and durably stores it before acknowledging.
- Metadata naming the peer as a provider is published only after that acknowledgement. If no eligible Full peer is available, the write fails and publishes no metadata.
- A peer must advertise support for durable blob storage before it can be selected for passthrough writes.

### 8. Streaming

- Files are chunked for content addressing anyway, so streaming reads = reading chunks in order.
- Local reads hit the native blob store. Passthrough reads always use an online Full peer. Both use the same read API.

### 9. OS integration (Linux mount)

- The filesystem must be mountable via FUSE on Linux, exposing the public API through standard syscalls.
- Kernel-level page caching is accepted as-is — not disabled, not specially optimized for. This is a deliberate simplification with one known cost: passthrough reads can serve stale kernel-cached data between explicit fetches, since the kernel page cache has no awareness of remote-triggered updates.
- File locking is **local-only**: FUSE lock/flock calls return an explicit `ENOSYS` rather than silently succeeding, with no distributed lock. Cross-device write conflicts are always resolved after the fact via the merge layer (§4), never prevented up front.
- File permissions and ownership (uid/gid/mode) are synced, each as its own field in the key-register subject to the same per-key LWW resolution as everything else.

## Consequences

**Positive**

- Transport is trivial to swap or mock (3 methods, no domain knowledge).
- Flat keyspace + per-key LWW avoids an entire class of tree-merge conflicts that a real directory structure would introduce.
- No dependency on a CRDT library — the merge model is small enough to reason about and test exhaustively (deterministic golden tests per plugin).
- Compiled-in plugins remove cross-device plugin-mismatch risk entirely for same-build fleets.
- Residency lets constrained devices avoid blob storage without weakening key/metadata synchronization.

**Trade-offs / risks**

- Without a CRDT library backing correctness, tombstone garbage collection, HLC correctness, and canonical/deterministic serialization are entirely this project's own responsibility rather than inherited from a library.
- Flat keyspace requires explicit empty-folder marker objects, and virtual-folder deletes are multi-key operations with no atomicity — a partial delete is a visible failure mode.
- Two peers creating the same key while offline still resolves via LWW — the losing write is simply overwritten (not preserved as a conflict copy); this is not considered a realistic scenario in practice.
- Plugin determinism is a hard constraint on every merge implementation; a non-deterministic plugin (e.g. one relying on hash-map iteration order) causes silent permanent divergence that's very hard to detect until long-separated replicas fail to converge.
- Passthrough files require an online Full peer for both reads and writes; a write remains unavailable until a Full peer durably acknowledges its blob.
- Accepting default kernel page caching means passthrough reads can be stale between fetches.
