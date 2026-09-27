# Domain Context

## Identity Provider (IdP)

The IdP is the OAuth 2.0 and OpenID Connect authority. It authenticates users, registers and validates OAuth clients, obtains consent, issues and verifies tokens, exposes OIDC metadata and JWKS, and manages signing-key metadata.

The IdP does not own device enrollment, trusted-device policy, storage resource management, or database/filesystem synchronization.

## Management Service

The Management Service is the control plane for an IdP installation. It owns management applications, roles, permissions, user-role assignments, device enrollment and revocation, trusted-device policy, and hosted-control-plane access.

It may use IdP repositories for application records because an application is an OAuth resource, but management authorization and lifecycle policy belong here.

## Bootstrap Service

The Bootstrap Service establishes the idempotent system baseline for a new installation. It creates or updates the built-in IdP and management applications and clients, the initial administrator and signing key, management permissions and roles, and an optional bootstrap device.

Bootstrap composes IdP and Management repositories but owns neither domain. It must be safe to run repeatedly.

## User

A User is the authenticated human subject. Its stable public identifier is the OAuth/OIDC `sub` claim. Profile, email, phone, password-verifier, and key records are stored separately from the core user record.

Raw passwords are never persisted or synchronized.

## Application

An Application is a logical product or resource identified by a stable URI. It groups one or more OAuth Clients and scopes storage resources owned by a user.

An application is not an OAuth client.

## OAuth Client

An OAuth Client is a concrete web, native, or machine integration for an Application. It has a `client_id`, redirect URIs, grant and response types, allowed scopes, and client authentication configuration. Multiple clients may access that application's resources when authorized by the same user.

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

Installation Setup establishes a new installation or joins an existing one. A
joining user authenticates with the existing IdP and explicitly authorizes the
new Device through its setup API. The joining Device obtains the IdP's durable database state through authenticated synchronization. Installation Setup completes only after that synchronization succeeds.
_Avoid_: Master setup, primary-node setup

## Device Setup

Device Setup configures and later edits the data an installation member stores locally after Installation Setup completes. Its completion is recorded in local device state.
_Avoid_: Setup Mode, device initialization

## Initial Administrator

The Initial Administrator is the username/password user whose supplied credentials establish administrative access when a new installation is created.
_Avoid_: Default admin, bootstrap admin

## Trusted Device

A Trusted Device is an approved, non-revoked Device eligible to participate in the mesh. Approval alone does not grant access to any database or filesystem.

## Hosted Control Plane

A Hosted Control Plane is the configured HTTP(S) authority a local runtime trusts for access-token verification and trusted-device discovery. Its issuer is configured locally, never derived from untrusted claims.

## Storage Resource

A Storage Resource is a database or filesystem owned by a User within an Application. Each has a distinct stable ID and may have a non-unique display name; one user/application may own multiple of either kind.

## Resource Catalog

The Resource Catalog is the synchronized record of storage resource identity, ownership, display names, discoverable grants, and deletion status. Discovery does not imply local possession of resource content; filesystem grant policy remains authoritative in the File System.

## Storage-Audience Access Token

A Storage-Audience Access Token is a standard IdP access token exchanged from an application client's user token for scoped Storage API access. It is not a separate token type or a single-use storage session.
_Avoid_: Storage Session, storage client token

## Resource Grant

A Resource Grant is owner-controlled permission for another subject to read or write an entire database or a path subtree of a filesystem. A grant for one resource or kind gives no rights to another.

## Filesystem ID

A Filesystem ID is the stable identity of one filesystem instance, independent of its local root path. It scopes filesystem authorization and synchronization.
_Avoid_: Vault ID

## Database ID

A Database ID is the stable identity of one application database, separate from the IdP/management control-plane database and every Filesystem ID.

## Resource Selection

Resource Selection is a device-local choice to retain and synchronize a resource its user can access. Deselecting removes only the local copy, not the resource itself.

## Residency

Residency is a device-local choice per filesystem path: Full or Passthrough. Full stores metadata and content locally; Passthrough stores metadata and reads content from an available Full peer but cannot write. Residency is not synchronized.

## File System

The File System is a local-first replicated storage engine for paths, metadata, grants, and file content. It is separate from application databases and IdP/management records.

## File Entry

A File Entry is metadata for one filesystem path, with a stable file ID, content revision, and provider information. Deletions are represented by synchronized metadata tombstones.

## Tombstone

A Tombstone is a synchronized deletion record that prevents a deleted file entry or storage resource from returning when an offline device reconnects.

## Repository Backend

A Repository Backend persists a service trait through the native replicated `ofdb` engine. CLI and desktop runtimes compose the native DB repositories directly.
