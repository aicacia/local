# ADR-002: Offline-First, Eventually Consistent Distributed File System

## Status

Accepted — supersedes ADR-001 (automerge/per-folder-doc design)

## Context

We need a Rust library for local file access that:

- Presents a native-OS-filesystem-like API (open, read, write, stream, list) designed so that binaries can mount it as a real filesystem on Linux via FUSE
- Works fully offline and online
- Auto-syncs across a network of peers over our iroh-based chain, using eventual consistency with no central coordinator
- Lets each device exclude applications or choose full/passthrough storage at namespace, folder, and file granularity
- Keeps the transport (iroh) swappable behind a trait
- Resolves all conflicts deterministically — any two replicas that have seen the same updates must converge to the same result, regardless of arrival order

FUSE mounting is a concern of consumer binaries, not of the library itself. The library exposes only the FS-like API; binaries that need a kernel mount implement the FUSE translation layer on top of that API.

## Decision

### 1. Layered architecture

Library layers:

```
┌─────────────────────────────┐
│   Public API (FS-like)       │  open, read, write, stream, list
│                              │  (shaped for FUSE compatibility)
├─────────────────────────────┤
│   Sync Engine                 │  owns the keyspace, merge rules, sync protocol
├─────────────────────────────┤
│   Metadata: flat key-value store │  path = key; per-key LWW-register
├─────────────────────────────┤
│   Storage Layer (native filesystem) │  file_id (UUID v7) addressed
├─────────────────────────────┤
│   Transport Trait (minimal)   │  send / broadcast / subscribe, no topics
├─────────────────────────────┤
│   iroh (default impl)         │  swappable
└─────────────────────────────┘
```

Binaries that need a kernel mount sit above the public API and provide a FUSE translation layer (Linux only). That layer is not part of the library.

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
- Every file has a stable identity independent of its key: a **UUID v7**. The key (path) is only how the file is accessed via the public API (and, in binaries that mount via FUSE, via the kernel) — renaming changes the key, never the id. UUID v7's time-ordered structure also makes it usable directly as a stable FUSE inode number when a binary chooses to expose the API through FUSE, considered a non-issue for the 99.9% of use cases (extremely low collision probability, monotonically-ish increasing so no reuse-after-remount concerns).
- No Merkle Search Tree is used. Key existence/pointer/tombstone state is resolved by a per-key **LWW-register**, not by a shared-root CRDT over the whole keyspace:
  - State-based CRDTs with a single shared document root are **not** used for the keyspace or for directory structure — each key is an independent root.
  - Diff-based (operation-based) CRDTs **are** allowed for file _content_ where a merge plugin needs them (see §4), provided each document has independent roots and merges via exchanged diffs/ops rather than shipping full document state.
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
    - Most file types default to **LWW** (last write wins on the content as a whole — one winning version of the file).
    - Some file types use custom merge logic via a **plugin**. Where a CRDT is used for content, it must be **diff-based (operation-based) with independent roots** — no shared document root across keys or across the keyspace, and peers exchange ops/diffs rather than full document state. Example: a file that is itself an Automerge (or similar) document is merged with that CRDT's own merge, not LWW, so concurrent edits are not silently dropped.
    - Plugins are **compiled into the codebase and version-controlled**, not installed independently per device — every replica on a given build has an identical plugin set, which removes cross-device plugin-mismatch divergence (cross-_build_-version skew remains a separate, open concern).
    - Dispatch is a simple enum keyed on file extension/mime type — no trait objects or dynamic registry needed:
      ```rust
      enum MergeStrategy {
          Lww,
          AutomergeDoc, // diff-based CRDT, independent root per file
          // new variants added here as new file types need custom merge behavior
      }
      ```
    - A plugin's merge output becomes a new value written back into the key-register under a fresh timestamp, so it wins the next LWW comparison — plugin merges feed the key layer rather than bypassing it. The CRDT document for a file is still an independent root keyed by that file's identity; it is never folded into a global shared-root CRDT.
    - **LWW is always the fallback**: if a replica encounters a file type whose plugin it doesn't have, it falls back to LWW deterministically rather than failing or blocking. In practice this fallback path is expected to be dormant — version negotiation for older builds lacking a plugin is out of scope, since all plugins discussed here ship in the initial release.
- **Determinism is a hard requirement** across the whole engine: HLC timestamp generation, tie-break ordering, and plugin merge functions must all be pure and reproducible. In particular, plugin code must avoid non-deterministic iteration order (e.g. Rust's `HashMap` — use `BTreeMap` or explicit sorting) and any dependence on wall-clock reads or platform-specific arithmetic. HLC timestamps are generated using an existing, audited Rust crate rather than a hand-rolled implementation.
- Tombstones are purged by a **configurable timed GC job**, once a day by default, rather than a precise per-tombstone watermark.
- The engine is also responsible for fetching blob content on demand (see passthrough, below), separate from metadata/key sync.
- The system is **local-first and reactive**: nodes act on changes as they arrive and never block waiting on a request/response round-trip to the network.

### 5. Content storage — single model, file_id addressed

One addressing scheme for all files, mutable or not:

- Every file already has a stable **UUID v7** `file_id` (see §3). Content is stored under that id (e.g. `.data/<file_id>`), never under a content hash as the primary key.
- Writes (overwrite or append) update the object for that `file_id` in place. Highly changing files (logs, etc.) do not allocate a new identity or a new content-hash path on every write.
- An optional content digest may be recorded in the key-register for integrity checks and change detection; it is metadata, not the storage locator. Dedup across different `file_id`s is out of scope.
- Chunking remains available for large files and streaming, but chunks are subordinate to the file_id object (not a global CA blob store keyed only by hash).
- Key-register entries point at content by `file_id` (plus size, optional digest, providers). Bytes are never inlined for full files.
- GC: when a key is tombstoned and aged past the GC window, the `.data/<file_id>` object is removed. Overwrites do not create orphan objects that need hash-based GC.
- Sync still decides _which version wins_ via per-key LWW or a content plugin; the storage layer materializes the winning bytes under the same `file_id`.

### 6. Device-local storage residency

Storage residency is device-local policy; it is not synchronized metadata.

- A device may exclude an entire Application. Exclusion applies to every Storage Namespace for that application and prevents local metadata and content storage from being created.
- An included application defaults to **Passthrough**.
- A device may set **Full** or **Passthrough** for a namespace, folder, or file. The longest matching path rule wins.
- The Global Identity Namespace is always Full and cannot be excluded.
- Full residency stores metadata and content locally. Passthrough stores metadata but no content bytes, and is not cached locally by the library (though, when a binary mounts via FUSE, the OS page cache may still cache recently-read pages — see §9).
- **Passthrough files are always read-only** on the local device. Writes are only possible under Full residency.
- Changing from Passthrough to Full fetches and verifies all selected content before marking the device as a provider. Changing from Full to Passthrough removes local references and garbage-collects only content unreferenced by other locally-full entries.

### 7. Passthrough reads

- Passthrough is **read-only**. Attempts to write, create, truncate, or otherwise mutate a passthrough file fail locally (e.g. `EROFS` / equivalent API error); no metadata is published and no content is uploaded.
- Every passthrough read goes to an online Full peer and streams content by `file_id` (and optional version/digest from the key-register).
- **Passthrough is unsupported** (reads fail with a clear error) when any of the following hold:
  - No peers are configured
  - No eligible Full peers are currently reachable
  - The network is explicitly set to offline
- A peer must advertise support for durable content storage (i.e. Full residency for that path) before it can be selected as a provider for passthrough reads.

### 8. Streaming

- Content is addressed by `file_id`. Large files may be chunked under that id; streaming reads = reading those chunks (or the single object) in order.
- Local reads hit the native `.data/<file_id>` store. Passthrough reads always use an online Full peer. Both use the same read API.

### 9. OS integration (Linux mount — binaries only)

- FUSE mounting is **not part of the library**. Consumer binaries that need a kernel-visible filesystem implement a FUSE translation layer on top of the library’s public FS-like API.
- The library API is deliberately shaped so that such a FUSE layer is straightforward: the same open / read / write / stream / list operations map cleanly to FUSE callbacks, and UUID v7 file ids can be used as stable inode numbers.
- When a binary does mount via FUSE on Linux:
  - Kernel-level page caching is accepted as-is — not disabled, not specially optimized for. This is a deliberate simplification with one known cost: passthrough reads can serve stale kernel-cached data between explicit fetches, since the kernel page cache has no awareness of remote-triggered updates.
  - File locking is **local-only**: FUSE lock/flock calls return an explicit `ENOSYS` rather than silently succeeding, with no distributed lock. Cross-device write conflicts are always resolved after the fact via the merge layer (§4), never prevented up front.
  - Passthrough paths should be presented as read-only to the kernel (e.g. mount flags or per-inode mode) so that write syscalls fail early rather than only at the library boundary.
- File permissions and ownership (uid/gid/mode) are synced by the library itself, each as its own field in the key-register subject to the same per-key LWW resolution as everything else. A FUSE layer simply surfaces those values to the kernel.

## Consequences

**Positive**

- Transport is trivial to swap or mock (3 methods, no domain knowledge).
- Flat keyspace + per-key LWW avoids an entire class of tree-merge conflicts that a real directory structure would introduce.
- Keyspace merge stays simple (per-key LWW); CRDTs are optional and scoped to content plugins only, and only as diff-based independent-root documents — no shared-root state CRDT over the filesystem.
- Content merge plugins (including diff-based CRDTs) remain small enough to reason about and test exhaustively (deterministic golden tests per plugin).
- Compiled-in plugins remove cross-device plugin-mismatch risk entirely for same-build fleets.
- Residency lets constrained devices avoid local content storage without weakening key/metadata synchronization.
- Single file_id-addressed storage avoids content-hash churn for logs and other highly mutable files; one path for all file types.
- Passthrough-as-read-only removes an entire class of remote-write failure modes and simplifies the provider/ack protocol.

**Trade-offs / risks**

- Tombstone garbage collection, HLC correctness, and canonical/deterministic serialization for the keyspace remain this project's responsibility (not inherited from a shared-root CRDT library). Diff-based content CRDTs used by plugins bring their own correctness constraints and must still satisfy the engine's determinism rules.
- Flat keyspace requires explicit empty-folder marker objects, and virtual-folder deletes are multi-key operations with no atomicity — a partial delete is a visible failure mode.
- Two peers creating the same key while offline still resolves via LWW — the losing write is simply overwritten (not preserved as a conflict copy); this is not considered a realistic scenario in practice.
- Plugin determinism is a hard constraint on every merge implementation; a non-deterministic plugin (e.g. one relying on hash-map iteration order) causes silent permanent divergence that's very hard to detect until long-separated replicas fail to converge.
- Passthrough is unsupported when offline, when no peers are configured, or when no Full peers are reachable — those devices cannot read passthrough content at all.
- When binaries mount via FUSE, accepting default kernel page caching means passthrough reads can be stale between fetches.
- Writes require Full residency on the local device; constrained devices that stay in Passthrough cannot create or modify files at all.
- Optional content digests in metadata must stay consistent with on-disk bytes; a mismatched digest is a local integrity failure, not a merge input.
- If append/tail-diff exchange is added later for hot files, it must still obey independent-root and determinism rules — easy to get wrong if treated like an ad-hoc log shipper.
