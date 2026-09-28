# Storage resource API contract (R1 draft)

The Storage API manages databases and filesystems; the Management API owns device enrollment, revocation, administrator storage limits, and resource selection/deselection for enrolled devices. Database and filesystem data/sync remain owned by `ofdb` and `file-system`. See [SECURITY-TRANSPORT.md](SECURITY-TRANSPORT.md) for the active authorization boundary. The database and filesystem CRUD routes below are implemented in `idp-server`; Management selection routes are not yet implemented.

## Identity and tokens

A resource is isolated to `(IdP token subject, application_id)`; `application_id` is derived from the validated OAuth client, not supplied by the request. Multiple clients of one application share that subject's resources. Other subjects and applications cannot discover, open or select them. Each namespace may own multiple databases and filesystems, each with its own stable, kind-specific opaque ID and optional non-unique display name. No file mode bits, groups, cross-user grants or per-folder access rules exist.

Clients use IdP-issued storage-audience access tokens from sign-in or token exchange. Tokens bind subject, application, audience and allowed API actions such as `read` and `write`; the Storage API checks the action and resource namespace on each request. A read-only credential cannot create or delete resources or change device configuration. No access token authorizes mesh sync: services sync selected resources independently of live user sessions.

## Storage resource routes

| Operation         | Route                                            | Result                                                |
| ----------------- | ------------------------------------------------ | ----------------------------------------------------- |
| Create database   | `POST /storage/databases` with optional `name`   | `201` new Database ID in caller's namespace           |
| List databases    | `GET /storage/databases`                         | caller's `(subject, application_id)` databases only   |
| Open database     | `GET /storage/databases/{database_id}`           | metadata if ID and namespace match                    |
| Delete database   | `DELETE /storage/databases/{database_id}`        | tombstone catalog entry for caller-owned database     |
| Create filesystem | `POST /storage/filesystems` with optional `name` | `201` new Filesystem ID in caller's namespace         |
| List filesystems  | `GET /storage/filesystems`                       | caller's `(subject, application_id)` filesystems only |
| Open filesystem   | `GET /storage/filesystems/{filesystem_id}`       | metadata if ID and namespace match                    |
| Delete filesystem | `DELETE /storage/filesystems/{filesystem_id}`    | tombstone catalog entry for caller-owned filesystem   |

Read/list/open require `read`. Create/delete require `write` (final action naming can follow existing IdP conventions). A kind-specific ID never authorizes the other kind. Unknown, deleted, or wrong-namespace IDs fail closed. Deletion tombstones the catalog entry and blocks subsequent opens; it does not itself revoke already-issued in-process database handles or remove synchronized device copies. Device deselection removes only that device's local copy. A Passthrough filesystem is read-only and does not store local file bytes.

## Management API boundary

The Management API enrolls/revokes/transfers device ownership and configures storage selection/administrator limits. Every device has a persisted owner: enrollment uses the authenticated subject, and pairing inherits the approved accepting device's owner. Records missing an owner fail closed; no owner is inferred or migrated. Selection is whole-resource, defaults to none, and is stored as synchronized control-plane policy keyed by device ID and resource kind/ID. An approved user-owned device may store its owner's namespace by default unless an administrator restricts it; selection still defaults to none. A device owner may choose resources from their own subject/application namespaces subject to admin limits; a read-only credential cannot mutate selection. Concurrent deselection beats selection; a subsequent deliberate selection can restore sync. Transfer clears selections and local copies before a new owner may select.

A signed-in owner can list their own resources to choose selections. Background services retain only selected-resource metadata, not a global catalog. Both peers check locally known device approval, admin allowance, selection, kind/ID and resource namespace before exchanging sync data; stale policy may temporarily permit sync until convergence. Before writing a selection, Management uses the existing `GET /storage/databases/{database_id}` or `GET /storage/filesystems/{filesystem_id}` API to validate kind, ID and namespace; it does not open or duplicate either catalog. The caller supplies a separate Storage read token for this lookup; Management verifies its subject matches the authorized device owner and its client maps to the requested `application_id` before using the GET result. The token is not persisted or used for sync. A successful GET with someone else's token is not authorization to select. Selection additionally requires the distinct Management permission and a matching device-owner subject; Storage `read` or `write` alone cannot change policy. Unknown resources, failed token/namespace binding and unavailable Storage APIs fail closed for selection, even offline. The owner can still deselect without contacting Storage. Background sync uses locally replicated selected-resource policy and no live Storage token. Administrators have separate authority. Exact selection route names remain to be specified; there are no Storage API device routes.
