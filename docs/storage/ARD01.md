# ADR-001: Web Access Server and Folder Authorization for the Distributed File System

## Status

Accepted

## Context

The distributed file system crate exposes an FS-like API (open, read, write, stream, list) with a local-first, reactive access model. We need a server that provides a fast connection endpoint for authenticating and authorizing clients, then streaming operations onto that crate.

Identities come from the IdP. The initial connection type is WebSocket.

Two constraints shape the access path:

1. Browser WebSocket clients cannot set custom headers on the handshake.
2. Folder permissions can change over the life of a long-lived connection.

The filesystem itself has no authorization model: `FileMeta.owner` and `FileMeta.group` are display metadata, not an ACL. Residency, providers, and replication identify device/content behavior and must not grant client access. Authorization must therefore be owned by `storage-service`, per storage namespace, and kept independent of replicated filesystem state.

## Decision

### Authorization model (owned by `storage-service`)

Authorization is owned by `storage-service`, per storage namespace, not by `file-system` metadata.

A namespace stores a durable authorization record for each normalized folder path:

- one owner IdP subject
- zero or more subject grants: `read` or `read-write`

Only one record may exist for a path. Paths use the filesystem's relative logical-path rules; the empty path is the namespace root.

To authorize `(subject, path, requested_access)`, select the longest folder record matching `path`. A folder record matches itself and descendants. If no record matches, deny. The owner has read-write access. A subject grant provides its declared access. A nested record replaces, rather than merges with, its ancestor's grants.

`storage-service` exposes one operation that reads the selected record and decides whether a requested access level is allowed while holding its authorization lock. RFC 8693 token exchange and standard refresh call that operation immediately before signing. Permission updates take the same lock, so no token is issued from an older decision after an update completes.

The initial implementation persists this namespace authorization data beside the namespace filesystem root in an atomically replaced JSON file. It is local server policy, is not included in deckv records, and is not synchronized through filesystem transports.

### OAuth 2.0 token exchange with authorization details

Clients first obtain an ordinary OAuth 2.0/OIDC access token. To connect to storage, they exchange it at the existing token endpoint using RFC 8693:

- `grant_type=urn:ietf:params:oauth:grant-type:token-exchange`
- the ordinary access token as `subject_token`
- the storage server as the `resource` / audience
- RFC 9396 `authorization_details` requesting one normalized folder and `read` or `write` actions

The token endpoint authenticates the client where applicable, validates the subject token, and asks `storage-service` for the current folder ACL decision immediately before signing the exchanged token. The result is an ordinary, short-lived signed OAuth access token, audience-bound to storage and narrowed to the granted authorization details. There is no storage-specific token type, issuer, or token endpoint.

Folder ownership and grants are durable local `storage-service` policy. They are not filesystem metadata and are not replicated.

### WebSocket handshake auth: signed JWT via query string

Because browser WebSocket clients cannot set an `Authorization` header (or any custom header) during the handshake, the signed JWT is passed as a query-string parameter on the WebSocket connection URL rather than as a header. The token must be signed as usual; transport via query string does not change its signature requirements.

### Token lifetime: short-lived, with standard refresh

The exchanged access token is short-lived. Refresh follows the existing OAuth refresh-token flow and re-evaluates the current `storage-service` ACL before issuing a replacement storage-audience access token. This keeps access aligned with current permissions without a separate storage session system.

### No additional permission re-check on the handshake itself

The WebSocket handshake does not perform a second, independent ACL lookup beyond validating the ordinary access token's signature, issuer, audience, authorization details, and expiry. A revoked or changed permission takes effect at the next token refresh rather than being enforced mid-connection.

### Server role: thin authenticated streaming endpoint

Once the token is validated the server streams (or proxies) the client's operations onto the crate's public API. It does not implement FUSE, does not own storage, and does not re-interpret permissions beyond the claims already present in the token.

## Consequences

- Authorization changes apply to exchanged tokens and standard refreshes; an already-issued token remains valid until expiry. The token lifetime therefore directly bounds how quickly a permission change takes effect — shorter lifetime means faster revocation propagation but more frequent refresh traffic.
- Nested folders are explicit policy boundaries. Grant inheritance is intentionally not merged.
- Query-string token transport means the token can appear in server access logs, proxy logs, and browser history. Combined with short expiry this limits exposure, but request logging on this server (and any reverse proxy/load balancer in front of it) should redact or avoid logging the full URL.
- The exchanged token's authorization details, rather than live per-operation ACL checks, define read-only or read-write access. Token issuance and refresh must use the current `storage-service` decision atomically with respect to permission updates.
- This design supports the WebSocket server without changing filesystem replication semantics.
- A distributed ACL requires a later ADR and replication design; it must not reuse provider/residency state.
- Supporting another connection type later (for example HTTP range requests) requires its own endpoint semantics. The storage audience and authorization details remain reusable; no `conn` claim is needed initially.

## Alternatives Considered

- **Use `FileMeta.owner`/`group` as permissions:** rejected; they cannot express subject grants or atomic scoped authorization and are replicated content metadata.
- **Store ACLs in deckv:** rejected for this server milestone; server policy does not need peer replication and requires its own conflict semantics.
- **Merge ancestor and child grants:** rejected; replacement is deterministic and avoids accidental privilege retention.
- **Authorization header for WebSocket auth:** rejected because browser WebSocket clients cannot set custom headers on the handshake; this would only work for non-browser clients and was ruled out in favor of a single consistent mechanism.
- **Per-operation live permission check:** rejected for this milestone in favor of short-lived RFC 8693 exchanged tokens and ACL re-evaluation during standard refresh; revisit if revocation latency proves too coarse.
- **Separate storage token issuer or opaque session token:** rejected. RFC 8693 produces a normal OAuth access token from the existing token endpoint.
- **Embedding FUSE semantics in the server:** rejected. FUSE is a binary-level concern that sits above the crate's API. The server only needs to validate tokens and stream onto that API.
