# Offline-First IdP and Storage

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE-MIT)
![Test Status](https://github.com/aicacia/rs-oauth/actions/workflows/ci.yml/badge.svg)

An OAuth 2.0 and OpenID Connect identity provider with application-scoped, offline-first storage.

Applications authenticate through OAuth/OIDC and access a filesystem scoped to the user and application. Data is stored locally, remains available offline, and synchronizes between approved devices.

## Services

- `idp-service` — OAuth/OIDC, users, clients, tokens, and signing keys.
- `management-service` — applications, permissions, devices, storage sessions, and tunnel authorization.
- `bootstrap-service` — initializes system applications, administrator, signing keys, and optionally a device.
- `storage-service` — provides authenticated filesystem access.
- `storage-server` — exposes storage over WebSocket.
- `file-system` — offline-first storage and synchronization.

## Storage

Storage is internally scoped by:

```text
(user ID, application ID)
```

Each application sees a user-specific `sub` derived from the user's public key. Clients belonging to the same application share the same filesystem for that user.

Applications authenticate with OAuth, exchange their bearer token for a short-lived storage session, and connect to `/storage`.

The storage API provides generic filesystem operations including read, write, append, delete, entry lookup, and directory listing.

## Offline-first filesystem

`file-system` stores data locally so reads and writes continue without network access.

When approved devices reconnect, changes synchronize automatically. Folder metadata uses Automerge, file content is transferred by hash, and deletes are propagated as tombstones.

Ordinary files use last-writer-wins metadata. `.automerge` and `.am` files use Automerge document merging.

## Devices

Each installation has a persistent device identity. Devices enroll and pair through `management-service`.

Approved devices synchronize application data peer-to-peer over Iroh. Connections use short-lived, single-use tunnel authorizations issued by the control plane.

Revoking a device closes its active tunnels and prevents future connections.

## Applications

Applications use OAuth/OIDC for authentication and the generic storage API for data.

Web, desktop, and mobile clients belonging to the same application can share the same user data while each device remains independently usable offline.

Applications define their own file formats and domain models; device management, peer connections, and synchronization are handled by the underlying services.
