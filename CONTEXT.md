# Domain Context

## Identity Provider (IdP)

The IdP is the OAuth 2.0 and OpenID Connect authority. It authenticates users, registers and validates OAuth clients, obtains consent, issues and verifies tokens, exposes OIDC metadata and JWKS, and manages signing-key metadata.

The IdP does not own device enrollment, trusted-device policy, storage sessions, storage scopes, filesystem synchronization, or tunnel policy.

## Management Service

The Management Service is the control plane for an IdP installation. It owns management applications, roles, permissions, user-role assignments, device enrollment and revocation, trusted-device policy, storage sessions, hosted-control-plane access, and tunnel authorization.

It may use IdP repositories for application records because an application is an OAuth resource, but management authorization and lifecycle policy belong here.

## Bootstrap Service

The Bootstrap Service establishes the idempotent system baseline for a new installation. It creates or updates the built-in IdP and management applications and clients, the initial administrator and signing key, management permissions and roles, and an optional bootstrap device.

Bootstrap composes IdP and Management repositories but owns neither domain. It must be safe to run repeatedly.

## User

A User is the authenticated human subject. Its stable public identifier is the OAuth/OIDC `sub` claim. Profile, email, phone, password-verifier, and key records are stored separately from the core user record.

Raw passwords are never persisted or synchronized.

## Application

An Application is a logical product or resource identified by a stable URI. It groups one or more OAuth Clients and defines the application side of an application-scoped Storage Namespace.

An application is not an OAuth client.

## OAuth Client

An OAuth Client is a concrete web, native, or machine integration for an Application. It has a `client_id`, redirect URIs, grant and response types, allowed scopes, and client authentication configuration. Multiple clients may belong to one application and share that application's storage namespace for the same user.

## OAuth Consent

OAuth Consent records a User's approval for a client, redirect URI, and scope set. It allows the authorization flow to determine whether interaction is required before issuing an authorization code.

## Authorization Code

An Authorization Code is a short-lived OAuth credential bound to its client, redirect URI, scopes, signing key, optional resource, PKCE challenge, and optional OIDC nonce. It is single-use: redeeming it durably marks it consumed so only one redemption succeeds.

## Access, ID, and Refresh Tokens

An Access Token authorizes an API request. An ID Token conveys authenticated OIDC identity claims to a client. A Refresh Token obtains replacement tokens under its original grant constraints. Tokens are signed by a Key and are validated against the configured issuer, audience, use, lifetime, and signature.

## Key

A Key is signing-key metadata: its entity owner, derivation relationship and path, name, state, and validity period. Its public JWK may be published through JWKS.

Private or derived key material is local secret state. It must not enter filesystem synchronization.

## Principal

A Principal is the entity represented by a signing key when the IdP issues or verifies a signed credential. Tunnel grants require a user principal.

## Role and Permission

A Permission is a named capability within an Application. A Role is a named collection of permissions within an Application. A user receives management authority through application-scoped role assignments.

The built-in management application URI is `idp-management`.

## Device

A Device is one installation's persistent transport endpoint identity, represented by its locally supplied name, public key, and reachable address. A device is pending, approved, or revoked. Device records are control-plane metadata, not application data.

A pairing request creates a pending device. An approved device signs the pairing approval payload. Revocation removes future trust but cannot erase data already copied to the revoked device.

## Reset Device

Reset Device removes one runtime's local setup state, local synchronized data, and device identity, returning it to Installation Setup. It attempts to revoke its approved device record remotely but proceeds when offline. It does not remove data from other devices.

## Installation Setup

Installation Setup establishes a new installation or joins an existing one. It completes only after the initial synchronization succeeds.
_Avoid_: Master setup, primary-node setup

## Device Setup

Device Setup configures and later edits the data an installation member stores locally after Installation Setup completes. Its completion is recorded in local device state.
_Avoid_: Setup Mode, device initialization

## Initial Administrator

The Initial Administrator is the username/password user whose supplied credentials establish administrative access when a new installation is created.
_Avoid_: Default admin, bootstrap admin

## Setup Token

A Setup Token is the one-time random credential a server generates at startup while Installation Setup is incomplete. It authorizes setup requests and is discarded after successful setup.

## Trusted Device

A Trusted Device is an approved, non-revoked Device eligible to synchronize a scope and participate in a tunnel. The trusted-device list is dynamic; runtimes refresh it and close tunnels to revoked peers.

## Hosted Control Plane

A Hosted Control Plane is the configured HTTP(S) authority a local runtime trusts for access-token verification, trusted-device discovery, storage-session issuance, and tunnel grants. A runtime must validate token and grant issuer claims against this configured URL, never a URL derived from untrusted claims.

## Storage Session

A Storage Session is a random, short-lived, single-use token exchanged from a valid storage-scoped OAuth access token. It carries a Storage Scope into the storage WebSocket handshake and is consumed when used.

## Storage Scope

A Storage Scope is the authorization context for application storage: `(user subject, application ID)`, plus the active principal, trusted devices, and bearer token needed by the runtime. It implements `StorageNamespace`.

Client IDs, device IDs, local paths, and user-supplied namespace strings are not storage scopes.

## Storage Namespace

A Storage Namespace is the stable logical partition identified by `(user subject, application ID)`. Each namespace maps to one local filesystem per node. All clients for the same user and application share that namespace.

## Storage Residency

Storage Residency is a device-local choice for a Storage Namespace: full or passthrough. Full residency stores metadata and blobs; passthrough synchronizes metadata and obtains blobs from peers when read and writes blobs directly to an available full-residency peer. Passthrough is the initial default. It may be set for a namespace, folder, or file; the most-specific rule applies. A device may exclude an entire Application, meaning it stores no metadata or blobs for any of that application's Storage Namespaces. Changes apply by fetching or deleting local data to match the selected residency.

## File System

The File System is the local-first replicated storage engine. It stores file content by hash and maintains Automerge metadata per folder. It synchronizes metadata and obtains missing content from peers through an authorized transport.

The filesystem stores application-scoped content; IdP and management records are not filesystem data.

## File Entry

A File Entry is replicated metadata for one path: content hash, size, known content providers, locality, deletion state, and merge strategy. A content hash identifies immutable file bytes.

## Tombstone

A Tombstone is a replicated deleted File Entry. It prevents a deleted path from returning when nodes synchronize. Tombstones remain part of the filesystem metadata until an explicit future compaction policy removes them.

## Merge Strategy

A Merge Strategy determines how concurrent file updates reconcile. Ordinary files use last-writer-wins metadata. `.automerge` and `.am` files use Automerge document merging.

Filesystem repositories should keep each mutable record in a stable path and rely on the documented strategy; they must not assume offline writes are globally serializable.

## Sync Document

A Sync Document is the persisted Automerge metadata document for one filesystem folder. It records that folder's File Entries and is exchanged with peers. Dirty-folder markers ensure local metadata changes survive restart before synchronization completes.

## Transport Tunnel

A Transport Tunnel is an isolated Iroh stream for one storage namespace and device pair. Iroh is a filesystem transport only; applications do not manage endpoint identities, tickets, peers, or synchronization directly.

## Tunnel Access Token

A tunnel uses the OAuth bearer access token issued for the storage scope. The receiver validates the standard access-token claims and signature, then checks that both endpoint identities are trusted devices before accepting the tunnel. Access tokens are not single-use OAuth grants.

## Vault ID

A Vault ID identifies the filesystem being synchronized. Its hash scopes the Iroh transport session; authorization is provided by the OAuth storage access token and trusted-device policy.

## Repository Backend

A Repository Backend persists a service trait through the native replicated `converge` engine. CLI and desktop runtimes compose the native DB repositories directly.
