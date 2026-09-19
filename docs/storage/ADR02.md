# ADR-002: Folder Authorization in Storage Service

## Status

Proposed

## Context

ADR-001 exchanges ordinary OAuth/OIDC access tokens for short-lived, storage-audience tokens scoped by authorization details. The filesystem has no authorization model: `FileMeta.owner` and `FileMeta.group` are display metadata, not an ACL. Residency, providers, and replication identify device/content behavior and must not grant client access.

## Decision

Authorization is owned by `storage-service`, per storage namespace, not by `file-system` metadata.

A namespace stores a durable authorization record for each normalized folder path:

- one owner IdP subject
- zero or more subject grants: `read` or `read-write`

Only one record may exist for a path. Paths use the filesystem's relative logical-path rules; the empty path is the namespace root.

To authorize `(subject, path, requested_access)`, select the longest folder record matching `path`. A folder record matches itself and descendants. If no record matches, deny. The owner has read-write access. A subject grant provides its declared access. A nested record replaces, rather than merges with, its ancestor's grants.

`storage-service` exposes one operation that reads the selected record and decides whether a requested access level is allowed while holding its authorization lock. RFC 8693 token exchange and standard refresh call that operation immediately before signing. Permission updates take the same lock, so no token is issued from an older decision after an update completes.

The initial implementation persists this namespace authorization data beside the namespace filesystem root in an atomically replaced JSON file. It is local server policy, is not included in deckv records, and is not synchronized through filesystem transports.

## Consequences

- This supports the ADR-001 WebSocket server without changing filesystem replication semantics.
- Authorization changes apply to exchanged tokens and standard refreshes; an already-issued token remains valid until expiry.
- Nested folders are explicit policy boundaries. Grant inheritance is intentionally not merged.
- A distributed ACL requires a later ADR and replication design; it must not reuse provider/residency state.

## Alternatives Considered

- **Use `FileMeta.owner`/`group` as permissions:** rejected; they cannot express subject grants or atomic scoped authorization and are replicated content metadata.
- **Store ACLs in deckv:** rejected for this server milestone; server policy does not need peer replication and requires its own conflict semantics.
- **Merge ancestor and child grants:** rejected; replacement is deterministic and avoids accidental privilege retention.
