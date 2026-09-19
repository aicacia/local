# Storage Web Access TODO

## Goal

Deliver the thin authenticated WebSocket endpoint defined by [ADR-001](ARD01.md): validate a short-lived, storage-audience OAuth access token with scoped authorization details, then stream only the permitted filesystem operations. Keep filesystem replication and transport details out of the server.

## 1. Unblock the storage boundary

- Migrate `storage-service` and `storage-server` from deleted filesystem APIs to `file_system::FileSystem<PeerId>` and its typed transport API.
- Remove obsolete codecs, raw-byte transports, and compatibility adapters; do not restore them in `file-system`.
- Make scoped roots explicit and configure the local server-owned paths as `Full` before writes.
- Update `idp-server` and the desktop caller only where required by the new storage-service API.

Tests:

- Storage service can create, read, list, and stream a scoped filesystem.
- A Passthrough scope rejects writes.

Validation per crate:

```sh
cargo test -p <crate>
cargo check -p <crate>
```

## 2. Define the storage-service permission boundary

- Implement ADR-002 folder-scoped ownership and `read` / `read-write` grants keyed by IdP subject.
- Select the longest matching folder rule; nested rules replace ancestor grants.
- Expose the smallest `storage-service` operation that decides access while sharing its lock with permission updates.
- Keep permissions separate from peer residency, content providers, replication transport, and filesystem metadata.

Tests:

- Owner, read-only grantee, and read-write grantee decisions.
- Nested folder rules and denied paths.
- A token cannot be issued from stale permission state after a permission update.

## 3. Exchange and validate standard scoped access tokens

- Add RFC 8693 token exchange to the existing OAuth/OIDC token endpoint; do not add a storage token type, issuer, endpoint, or opaque session.
- Require the normal subject access token, bind the result to the storage resource/audience, and preserve normal client authentication or public-client PKCE requirements.
- Use RFC 9396 `authorization_details` to request and return one normalized folder with `read` or `write` actions.
- Call the current `storage-service` authorization decision immediately before signing the exchanged token.
- Re-evaluate the ACL in the existing refresh-token flow before minting a replacement storage-audience token.
- Use short expiry and redact query strings containing tokens from application and proxy access logs.

Tests:

- A valid exchanged storage-audience token with read or write authorization details validates.
- Expired, invalidly signed, wrong-audience, and wrongly scoped tokens fail.
- Permission denial prevents exchange and refresh after revocation fails.

## 4. Implement the WebSocket operation endpoint

- Authenticate the WebSocket upgrade from the signed OAuth access token query parameter; remove the first-frame opaque-session authentication flow.
- Validate issuer, expiry, storage audience, and RFC 9396 authorization details before upgrade.
- Map each typed request to the public scoped filesystem API.
- Enforce the authorization-details folder boundary and actions before dispatch: `read` permits read/list/stream; `write` additionally permits mutations.
- Keep request/response framing and filesystem transport separate; no FUSE or iroh protocol in the server.
- Return typed client-safe errors without exposing filesystem roots or internal peer details.

Tests:

- A scoped read succeeds only within its folder, with component-boundary matching.
- Read authorization rejects write, append, delete, rename, and directory creation.
- Write authorization can mutate only its scoped folder.
- Invalid, expired, or wrong-audience query tokens are rejected before upgrade.

## 5. Add refresh and operational limits

- Refresh through the existing OAuth refresh-token flow only after re-evaluating current permissions.
- Close or reject requests after the exchanged access token expires when refresh fails.
- Set message-size, concurrency, and stream backpressure limits.
- Ensure logs and metrics identify a request without recording JWTs or file content.

Tests:

- Revoked access fails during standard refresh.
- Expired exchanged tokens cannot continue mutating.
- Oversized or malformed frames fail safely.

## Deferred filesystem work

Do not start these for the web-access-server milestone:

- content merge plugins
- chunked content transfer
- coordinated content/tombstone garbage collection
- FUSE
- additional transports beyond the tested typed iroh adapter

## Milestone validation

```sh
cargo fmt --check
cargo test -p storage-service
cargo test -p storage-server
cargo test -p idp-server
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```
