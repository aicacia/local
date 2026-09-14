# Local-First IdP and Storage

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE-MIT)
![Test Status](https://github.com/aicacia/rs-oauth/actions/workflows/ci.yml/badge.svg)

An OAuth 2.0 and OpenID Connect authority with local-first, application-scoped
storage. Applications are ordinary OAuth clients: they do not own device keys,
peer addresses, filesystem roots, or synchronization.

See [`CONTEXT.md`](CONTEXT.md) for the domain glossary.

## Service boundaries

### IdP

`idp-service` owns OAuth and OIDC behavior:

- OAuth client registration and validation;
- authorization, consent, PKCE, and authorization codes;
- access, ID, and refresh token issuance and validation;
- user identity, profile data, signing-key metadata, OIDC metadata, and JWKS.

It does not own device enrollment, trusted-device policy, storage sessions, or
tunnel authorization.

### Management

`management-service` is the installation control plane. It owns:

- management applications, roles, permissions, and user-role assignments;
- device enrollment, pairing approval, revocation, and trusted-device policy;
- one-use storage sessions and storage scopes;
- hosted-control-plane access and tunnel authorization.

### Bootstrap

`bootstrap-service` creates the idempotent system baseline: built-in IdP and
management applications and clients, the initial administrator and signing
key, management access, and an optional bootstrap device.

### Repository backends

Each owning service exposes its own repository implementations through features:

- `idp-service`: `fs` and `libsql`;
- `management-service`: `fs` and `libsql`.

The filesystem backend uses `file-system` and is intended for synchronized,
local-first state. The LibSQL backend remains available during migration.

## Storage

`storage-model` defines the generic storage protocol, `storage-service` runs it
against `file-system`, and `storage-server` exposes its WebSocket API. Storage
does not know an application's domain model.

Applications use this flow:

1. Sign in through OAuth using their own client ID.
2. Exchange the storage-scoped bearer token for a one-use, short-lived storage
   session at `POST /storage/sessions`.
3. Authenticate `/storage` with that session token.
4. Send generic filesystem requests: read, write, append, delete, entry, and
   list.

A storage namespace is derived internally from:

```text
(user subject, application ID)
```

Client IDs are not namespaces. Web, desktop, and mobile clients belonging to
the same application share a filesystem for the same user.

## Filesystem synchronization

The filesystem library can also be used without any storage service or network
transport. Native applications can open a local filesystem directly and add a
transport-aware integration when replication is needed:

```rust,ignore
use file_system::{LocalFileSystem, NativeStorage};

let storage = NativeStorage::new("./data")?;
let file_system = LocalFileSystem::open_local(storage).await?;
file_system.write("notes/today.txt", b"hello").await?;
```

The local API owns file content and metadata. Replication is an integration
concern; Iroh, namespace identity, residency policy, and authorization remain
outside the library-first filesystem API.

Each storage namespace has one local filesystem per node. The runtime derives
its local directory; applications cannot provide paths.

```text
vaults/
  <subject>/
    <application-id>/
```

`file-system` replicates folder metadata with Automerge and transfers missing
content by hash. Deletes are tombstones, so a deleted file stays deleted when
peers reconnect. Ordinary files use last-writer-wins metadata; `.automerge`
and `.am` files use Automerge document merging.

Filesystem repositories store synchronized domain records as files. They use
random positive IDs so offline nodes do not collide. Raw passwords and private
or derived key material remain local and are never synchronized.

## Devices and transport

Each installation owns a persistent device endpoint identity. Iroh is only the
`file-system` transport, never an application API.

For an approved device pair and one namespace, the runtime creates an isolated
stream:

```text
(subject, application ID, local device, remote device)
```

A tunnel authorization is short-lived and single-use. It binds the user,
application, vault hash, both device public keys, issuer, and expiration. The
control plane issues it only for trusted devices; the receiver verifies and
consumes it before accepting a tunnel.

The trusted-device list is dynamic. A runtime refreshes it from its configured
control plane, closes revoked peers' tunnels, and synchronizes approved peers
through `FileSystem::sync_peer`.

## Hosted control-plane trust

A local runtime may use a configured `control_plane_uri`. It trusts only that
HTTP(S) authority, validates access tokens and tunnel grants against its JWKS,
and never derives an issuer URL from untrusted claims.

The local HTTPS server and hosted deployment share the `/storage` WebSocket
contract. Native applications use the local server; hosted applications may use
the hosted URL directly.

## Application responsibilities

An application:

- performs OAuth and retains its access token;
- uses the shared storage client and generic storage API;
- validates and interprets its own file contents;
- renders its own domain model.

An application must not manage device identities, Iroh tickets, filesystem
roots, peer allowlists, or direct synchronization.

## Goal

Approved devices converge application-scoped files after offline work and
reconnection:

1. A device enrolls with its persistent public key and address.
2. An approved device confirms its pairing request.
3. Trusted devices synchronize the same namespace through authorized Iroh
   transport.
4. Revocation closes active tunnels, rejects future grants, and prevents
   reconnects.

Revocation cannot erase data already copied to a revoked device.

## Required guarantees

- Paths cannot escape their authenticated storage namespace.
- A user or application cannot access another namespace.
- Invalid sessions, bearer tokens, paths, peers, and tunnel grants are rejected.
- Approved nodes converge writes, concurrent updates, and tombstones after
  reconnecting.
- Revoked devices cannot reconnect.
