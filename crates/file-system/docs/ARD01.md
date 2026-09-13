# ARD: Eventual-Consistency Distributed File System Crate

## Status

Accepted

## Context

We need a Rust library for local file access that:

- Works fully offline and online
- Auto-syncs across a network of peers (our iroh-based chain)
- Uses eventual consistency — no central coordinator
- Lets each device exclude applications or choose full/passthrough storage at namespace, folder, and file granularity
- Keeps the transport (iroh) swappable behind a trait

## Decision

### 1. Layered architecture

```
┌─────────────────────────────┐
│   Public API (FS-like)      │  open, read, write, stream, list
├─────────────────────────────┤
│   Sync Engine                │  owns automerge docs + sync protocol + merge rules
├─────────────────────────────┤
│   Metadata: Automerge (per folder) │  file tree entries, CRDT
├─────────────────────────────┤
│   Storage Layer (native filesystem) │  blobs (full files) + automerge doc persistence
├─────────────────────────────┤
│   Transport Trait (minimal)   │  send / broadcast / subscribe, no topics — just moves bytes
├─────────────────────────────┤
│   iroh (default impl)         │  swappable
└─────────────────────────────┘
```

### 2. Transport trait — minimal, dumb pipe

The transport's only job is moving bytes between peers. It knows nothing about files, sync, or automerge — and it doesn't expose topics/channels either, since most transports won't have that concept. Any topic/routing needed internally (e.g. iroh gossip topics) stays inside the impl.

```rust
trait Transport {
    type PeerId;

    async fn send(&self, peer: Self::PeerId, data: Bytes);
    async fn broadcast(&self, data: Bytes);
    async fn subscribe(&self) -> Stream<(Self::PeerId, Bytes)>;
}
```

iroh is the default implementation. Swapping to a different transport later means implementing these three methods — nothing else in the library changes.

### 3. Metadata layer — Automerge, scoped per folder

- Each folder is its own automerge document. Avoids one giant global doc; scales better since only relevant folders need to sync.
- A folder's automerge doc holds one entry per file:
  - `name`
  - `hash` (content address of the blob)
  - `size`
  - `providers` (known peers holding the blob)
  - `local: bool` (whether this device has the blob on disk)
  - `merge_strategy` (tag used for per-file-type conflict handling)
- Automerge gives us hash-linked change history and a built-in sync protocol for free — no custom diffing logic needed.

### 4. Sync engine — the library's responsibility

- Owns the automerge documents and drives automerge's sync protocol.
- Sends/receives automerge sync messages over the `Transport` trait (transport just shuttles opaque bytes).
- Two separate layers of conflict handling, not one:
  - **Directory/metadata conflicts** (entries added/removed/renamed, field edits within a folder doc): handled entirely by automerge's built-in CRDT merge. No custom logic here — this is automerge's job, full stop.
  - **File conflicts** (two peers both edit the _content_ of the same file while offline): resolved according to the file's **type**, not a config setting. `merge_strategy` is a property of the file type itself:
    - Most file types default to **LWW** (last write wins on the blob as a whole).
    - Some file types have their own merge logic — e.g. a file that is itself an automerge document must be merged using automerge's own merge, not LWW, or edits would be silently dropped.
    - New file types (and their merge behavior) should be addable later without changing the sync engine's core loop.
    - Dispatch is a simple enum, not a struct per file type — files carry no per-type structure, just a name/hash/mime-type/extension in their metadata entry. The enum is keyed off extension or mime type and maps to a merge function:
      ```rust
      enum MergeStrategy {
          Lww,
          AutomergeDoc,
          // new variants added here as new file types need custom merge behavior
      }
      ```
      Looking up the strategy is a simple match on file extension/mime type — no trait objects or registry needed.
- Also responsible for fetching blob content on demand (see passthrough, below), separate from metadata sync.

### 5. Blob storage

- Files are content-addressed (hashed) and chunked, enabling streaming and integrity checks.
- Full files are stored as native filesystem blobs under `.blobs/<hash>`.
- Folder documents are persisted under `.metadata/`.
- Automerge docs never contain blob bytes — only metadata pointing at them.

### 6. Device-local storage residency

Storage residency is device-local policy; it is not synchronized metadata.

- A device may exclude an entire Application. Exclusion applies to every Storage Namespace for that application and prevents local metadata and blob storage from being created.
- An included application defaults to **Passthrough**.
- A device may set **Full** or **Passthrough** for a namespace, folder, or file. The longest matching path rule wins.
- The Global Identity Namespace is always Full and cannot be excluded.
- Full residency stores metadata and blobs locally. Passthrough stores metadata but no blob bytes.
- Changing from Passthrough to Full fetches and verifies all selected blobs before marking the device as a provider. Changing from Full to Passthrough removes local references and garbage-collects only blobs unreferenced by other locally-full entries.

### 7. Passthrough reads and writes

- Every passthrough read goes to an online Full peer and streams the blob by hash. Passthrough content is not cached locally.
- A passthrough write requires an online Full peer. The writer uploads the blob to that peer; the peer verifies and durably stores it before acknowledging.
- Metadata naming the peer as a provider is published only after that acknowledgement. If no eligible Full peer is available, the write fails and publishes no metadata.
- A peer must advertise support for durable blob storage before it can be selected for passthrough writes.

### 8. Streaming

- Files are chunked for content addressing anyway, so streaming reads = reading chunks in order.
- Local reads hit the native blob store. Passthrough reads always use an online Full peer. Both use the same read API.

## Consequences

**Positive**

- Transport is trivial to swap or mock (3 methods, no domain knowledge).
- Automerge removes the need to hand-build CRDT merge logic and sync diffing.
- Per-folder docs keep sync traffic scoped and manageable as the file tree grows.
- Residency lets constrained devices avoid blob storage without weakening metadata synchronization.

**Trade-offs / risks**

- Moving a file/folder across automerge doc boundaries is a delete in the source folder doc + create in the destination folder doc — not an atomic move. Two peers moving the same file differently while offline could result in odd end states (e.g. both, or neither) that the app layer should be aware of.
- `merge_strategy` logic must stay deterministic across peers (all peers must resolve the same file conflict the same way, or content will keep diverging).
- Passthrough files require an online Full peer for reads and writes. A write remains unavailable until a Full peer durably acknowledges its blob.
