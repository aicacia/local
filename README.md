# Local First IdP and Storage

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE-MIT)
![Test Status](https://github.com/aicacia/rs-oauth/actions/workflows/ci.yml/badge.svg)

OIDC/OAuth 2.x authority with local-first, application-scoped
storage. Applications remain ordinary OAuth clients. They do not own device
keys, peer addresses, or sync code.

## Boundaries

### IdP

Idp is the identity and device-control plane.

- Issues OAuth access tokens and maps a token's client to its application.
- Stores user device membership in SQLite.
- Approves and revokes device public keys and addresses.
- Exposes the trusted-device list and short-lived tunnel authorizations.
- Is the only source of user, application, and device authorization.

### Storage

`storage-model` defines the generic storage protocol, `storage-service`
executes that protocol against `file-system`, and `storage-server` exposes the
WebSocket API. Storage does not understand password-manager records or any
other application's domain model.

Applications use this flow:

1. Sign in with OAuth using their own client ID.
2. Exchange the bearer token for a one-use, short-lived storage session at
   `POST /storage/sessions`.
3. Authenticate a WebSocket to `/storage` with that session token.
4. Send generic file-system requests: read, write, append, delete, entry, and
   list.

The storage scope is derived internally as:

```text
(token subject, application ID)
```

Client IDs are not storage namespaces. Multiple web, desktop, and mobile
clients for one application share one application-scoped filesystem.

### Filesystem

Each `(subject, application ID)` scope has one local filesystem. LIDP derives
its storage directory; applications never supply local paths.

```text
vaults/
  <subject>/
    <application-id>/
```

### Devices and transport

Each LIDP installation owns one persistent device endpoint identity. Iroh is
used only as a `file-system` transport, never as an application API.

For an active scope and approved peer, LIDP creates one isolated stream:

```text
(subject, application ID, local device, remote device)
```

Tunnel setup validates a short-lived, single-use authorization bound to the
subject, application, vault hash, both device public keys, and expiration.
There is no cross-user or cross-application routing inside a tunnel.

The device allowlist is dynamic. The local runtime refreshes it from the
configured hosted LIDP authority using the authenticated access token, removes
revoked peers, closes their active tunnels, and synchronizes each approved
peer through `FileSystem::sync_peer`.

### Hosted authority trust

A local LIDP runtime can use a configured `control_plane_uri`. It trusts only
that configured authority, verifies bearer tokens and tunnel grants against
its JWKS, and does not derive an issuer URL from untrusted token claims.

The local HTTPS server and hosted deployment use the same `/storage` WebSocket
contract. Native applications use the local server; hosted applications use
the hosted URL directly.

## Application responsibilities

An application:

- performs OAuth and keeps its access token;
- uses the shared storage client and generic storage API;
- validates and interprets its own file contents;
- renders its own domain model.

An application must not contain Iroh identities or tickets, filesystem roots,
device allowlists, or direct synchronization code.

## Final Goal

The completed system provides application-scoped files that follow an
authenticated user across approved devices:

1. A new device enrolls with its persistent public key and address.
2. An existing approved device approves the enrollment.
3. Approved devices synchronize the same filesystem through scoped Iroh
   transport.
4. Revocation removes the device from the hosted allowlist, closes active
   tunnels, rejects future grants, and prevents reconnects.

Revocation cannot erase data that a revoked device already copied.

## Completion Criteria

- Storage paths cannot escape their authenticated scope.
- A user or application cannot access another scope.
- Invalid session tokens, bearer tokens, record paths, and peer identities are
  rejected.
- Two and three approved devices converge writes, concurrent updates, and
  deletions after reconnecting.
- Deleted data remains deleted after synchronization through tombstones.
- Revoked devices cannot reconnect.
