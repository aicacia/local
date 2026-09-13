# Setup and Device Identity TODO

- [x] Record agreed terminology in `CONTEXT.md`.
- [x] Amend `crates/file-system/docs/ARD01.md` for application exclusion, hierarchical residency, and online passthrough writes.
- [x] Add persisted local setup state, per-installation identity ID, and startup-only setup token.
- [x] Store all native Iroh device identities in the OS keyring; remove file-key configuration and persistence.
- [x] Remove `BootstrapConfig.is_master`, `device_name`, and `admin_*`; accept setup-supplied bootstrap input instead.
- [x] Stop automatic baseline creation and add token-protected setup APIs shared by desktop and server.
- [x] Add desktop Installation Setup and Device Setup screens, including initial admin/device input and residency editing.
- [ ] Implement Join completion after Global Identity Namespace sync and local device setup completion state.
- [x] Implement Reset Device with best-effort self-revocation and local cleanup.
- [x] Add persisted application exclusion and hierarchical full/passthrough residency policy.
- [x] Enforce excluded applications before storage namespace creation.
- [x] Implement residency transitions and garbage collection of evicted blobs.
- [x] Implement online passthrough writes with durable full-peer acknowledgement.
- [ ] Add unit/integration coverage and run formatting, targeted tests, workspace feature tests, and clippy.

## Required Foundations / Blockers

- [ ] Global Identity Namespace bootstrap and synchronization: define a fixed, always-Full runtime and grant protocol; Join must not advance from Installation until the joining device has synchronized and verified its approved device record.
  - [x] Define a fixed domain-separated vault ID and dedicated local runtime root outside application residency policy.
  - [x] Define a lossless canonical replicated identity/management record model. The canonical row contract covers every IdP/management SQL column, including blobs, credentials, enrollment, authorization-code consumption, links, IDs, and timestamps; derived private keys remain local.
  - [x] Define an atomic replicated import/activation boundary. Revisions stage complete hash-bound canonical records and manifest before a local atomic active-revision marker is written after full validation.
  - [x] Add global bootstrap grants bound to both endpoints, nonce, expiry, and the global vault; consume each grant once.
  - [ ] Make the Global Identity revision authoritative for live IdP and management reads/writes. Local SQL is only a strictly derived, activated read cache; it must never publish or authorize state.
    - [x] Add an atomic cache projector that accepts only manifest/path/hash-verified canonical rows and commits the replacement plus applied revision marker in one LibSQL transaction.
    - [x] Export complete deterministic canonical snapshots from every current identity/management table, including type and relationship validation.
    - [x] Add a canonical revision writer: apply each domain command to the authoritative snapshot, validate it, stage/activate a complete new revision, then project the local cache.
    - [ ] Route bootstrap, IdP, and management mutations through the canonical revision writer; remove direct `LibSql*Repo` authority writes.
    - [ ] Gate IdP and management reads, including tunnel authorization, on the cache revision matching the verified active Global Identity revision.
    - [ ] Construct and refresh the Global Identity runtime in the IdP server, management server, and desktop composition roots before serving requests.
  - [ ] Synchronize the global namespace and verify the local approved device record before advancing Installation Setup.
    - [ ] Compose global bootstrap-grant authorization with ordinary application-vault authorization and allow both pairing endpoints before creating the Global tunnel.
    - [ ] Connect the dedicated Global Identity `ScopedIrohTransport`, explicitly synchronize its filesystem, activate the granted revision, and project its cache.
    - [ ] Require an approved, unrevoked device record matching the local endpoint ID before advancing `Installation` to `Device`.
- [ ] Setup Join API and transport: add a setup-token-protected join flow that carries a usable Iroh endpoint address, consumes pairing offers, receives a single-use bootstrap grant, and resumes safely after failure.
  - [x] Define pairing offer/reply payloads carrying the joiner endpoint address, expected endpoint ID, nonce, bootstrap grant, and target global revision.
  - [x] Persist only retry-safe public join metadata; retain `Installation` for all failed, expired, or incomplete attempts.
  - [ ] Consume pairing offers on an approved device, verify the remote endpoint ID, register/approve the joining device through the canonical revision writer, and issue one endpoint-bound grant.
  - [ ] On the joining device, validate the reply, establish the Global tunnel, synchronize/activate/verify the approved record, then return `SetupJoinState::Complete`.
  - [ ] Add a two-endpoint integration test covering successful Join and retries plus rejection tests for endpoint mismatch, replayed/expired grants, and missing/pending/revoked approval.
- [x] Device residency API: persisted device-wide Full/Passthrough default, setup-token-protected local API, and desktop Device Setup editor. Scoped rules override the default once a namespace exists.
- [x] Residency placement operations: add filesystem materialize/evict operations, apply hierarchical scoped policy to opened filesystems, and retain reference-aware GC for evicted and deleted blobs.
- [x] Durable passthrough upload protocol: idempotent blob upload, durable NativeStorage persistence, receiver Full-residency admission, and hash-bound acknowledgement before metadata publication.
- [x] Hosted reset self-revocation: signed control-plane self-revocation now precedes the existing bounded local cleanup flow.
