# ADR-003: Web Access Server for the FUSE Distributed File System

## Status
Proposed

## Context
The FUSE-based distributed file system crate supports namespace/folder/file
-granular permissions and a local-first, reactive access model. We need a
server that exposes this file system over the web, authenticating and
authorizing requests using identities from the IdP (ADR-001), and initially
supporting WebSocket as the connection type.

Two constraints shape this design: WebSocket connections in browsers can't
carry custom headers on the handshake, and folder permissions can change
over the life of a long-lived connection.

## Decision

**Token scope claims: folder path, access level, connection type.**
Access tokens issued for this server carry:
- `folder`: the specific path/folder the token is scoped to
- `access`: read-only or read-write
- `conn`: connection type (currently only `websocket`)

Folder-level owner/permission identifiers are stored on the file-system
side (per ADR-002) and may use any identifier scheme; this server is
responsible for mapping an authenticated IdP subject to the relevant
file-system permission entries when issuing a scoped token.

**WebSocket handshake auth: signed JWT via query string.**
Because browser WebSocket clients cannot set an `Authorization` header (or
any custom header) during the handshake, the signed JWT is passed as a
query-string parameter on the WebSocket connection URL rather than as a
header. The token must be signed as usual; transport via query string does
not change its signature requirements.

**Token lifetime: short-lived, with refresh.**
Tokens are issued with a short expiry and refreshed rather than being
long-lived. This is the primary mechanism for keeping access aligned with
current permissions, given point below.

**No additional permission re-check on the handshake itself.**
The WebSocket handshake does not perform a second, independent
authorization check beyond validating the token's signature, scope, and
expiry. The short-lived-plus-refresh token model is treated as sufficient
enforcement: a revoked or changed permission takes effect at the next
token refresh rather than being enforced mid-connection.

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
- Since access level is a token claim (read-only vs. read-write) rather
  than checked live against the file system on every operation, the
  token issuance step must be the single source of truth at issue time;
  any race between a permission change and an in-flight token issuance
  needs to resolve in favor of the file system's current state.
- Supporting additional connection types later (e.g. plain HTTP range
  requests) should be able to reuse the same claim shape by adding new
  `conn` values, without changing the `folder`/`access` claims.

## Alternatives Considered
- **Authorization header for WebSocket auth**: rejected because browser
  WebSocket clients cannot set custom headers on the handshake; this
  would only work for non-browser clients and was ruled out in favor of
  a single consistent mechanism.
- **Per-operation live permission check**: considered for stronger
  consistency, but rejected for the handshake step specifically in favor
  of the simpler short-lived-token model; revisit if revocation latency
  proves too coarse in practice.
