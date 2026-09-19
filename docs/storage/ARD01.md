# ADR-001: Web Access Server for the Distributed File System Crate

## Status

Proposed

## Context

The distributed file system crate (see ADR-002) exposes an FS-like API
(open, read, write, stream, list) with namespace/folder/file-granular
permissions and a local-first, reactive access model. We need a server
that provides a fast connection endpoint for authenticating and
authorizing clients, then streaming operations onto that crate.

Identities come from the IdP. The initial connection type is WebSocket.

Two constraints shape this design:

1. Browser WebSocket clients cannot set custom headers on the handshake.
2. Folder permissions can change over the life of a long-lived connection.

## Decision

**OAuth 2.0 token exchange with authorization details.**
Clients first obtain an ordinary OAuth 2.0/OIDC access token. To connect to
storage, they exchange it at the existing token endpoint using RFC 8693:

- `grant_type=urn:ietf:params:oauth:grant-type:token-exchange`
- the ordinary access token as `subject_token`
- the storage server as the `resource` / audience
- RFC 9396 `authorization_details` requesting one normalized folder and
  `read` or `write` actions

The token endpoint authenticates the client where applicable, validates the
subject token, and asks `storage-service` for the current folder ACL decision
immediately before signing the exchanged token. The result is an ordinary,
short-lived signed OAuth access token, audience-bound to storage and narrowed
to the granted authorization details. There is no storage-specific token type,
issuer, or token endpoint.

Folder ownership and grants are durable local `storage-service` policy as
specified by ADR-002. They are not filesystem metadata and are not replicated.

**WebSocket handshake auth: signed JWT via query string.**
Because browser WebSocket clients cannot set an `Authorization` header (or
any custom header) during the handshake, the signed JWT is passed as a
query-string parameter on the WebSocket connection URL rather than as a
header. The token must be signed as usual; transport via query string does
not change its signature requirements.

**Token lifetime: short-lived, with standard refresh.**
The exchanged access token is short-lived. Refresh follows the existing OAuth
refresh-token flow and re-evaluates the current `storage-service` ACL before
issuing a replacement storage-audience access token. This keeps access aligned
with current permissions without a separate storage session system.

**No additional permission re-check on the handshake itself.**
The WebSocket handshake does not perform a second, independent ACL lookup
beyond validating the ordinary access token's signature, issuer, audience,
authorization details, and expiry. A revoked or changed permission takes effect
at the next token refresh rather than being enforced mid-connection.

**Server role: thin authenticated streaming endpoint.**
Once the token is validated the server streams (or proxies) the client's
operations onto the crate's public API. It does not implement FUSE, does
not own storage, and does not re-interpret permissions beyond the claims
already present in the token.

## Consequences

- Query-string token transport means the token can appear in server
  access logs, proxy logs, and browser history. Combined with short
  expiry this limits exposure, but request logging on this server (and
  any reverse proxy/load balancer in front of it) should redact or avoid
  logging the full URL.
- Because there's no re-check at handshake time beyond signature/scope/
  expiry, a permission revoked mid-session remains effective (from the
  server's point of view) until the current token expires and a refresh
  is attempted. The token lifetime therefore directly bounds how quickly
  a permission change takes effect — shorter lifetime means faster
  revocation propagation but more frequent refresh traffic.
- The exchanged token's authorization details, rather than live per-operation
  ACL checks, define read-only or read-write access. Token issuance and refresh
  must use the current `storage-service` decision atomically with respect to
  permission updates.
- Supporting another connection type later (for example HTTP range requests)
  requires its own endpoint semantics. The storage audience and authorization
  details remain reusable; no `conn` claim is needed initially.

## Alternatives Considered

- **Authorization header for WebSocket auth**: rejected because browser
  WebSocket clients cannot set custom headers on the handshake; this
  would only work for non-browser clients and was ruled out in favor of
  a single consistent mechanism.
- **Per-operation live permission check**: rejected for this milestone in
  favor of short-lived RFC 8693 exchanged tokens and ACL re-evaluation during
  standard refresh; revisit if revocation latency proves too coarse.
- **Separate storage token issuer or opaque session token**: rejected. RFC 8693
  produces a normal OAuth access token from the existing token endpoint.
- **Embedding FUSE semantics in the server**: rejected. FUSE is a
  binary-level concern that sits above the crate's API (ADR-002). The
  server only needs to validate tokens and stream onto that API.
