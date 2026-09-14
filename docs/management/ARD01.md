# ADR-002: IdP Management Service and App

## Status
Proposed

## Context
The OIDC/OAuth2 IdP (ADR-001) needs an administrative surface for managing
relying parties, users, and their keys. Because the IdP is the sole owner
of public keys and per-client derived-key mappings, and because folder-level
permissions in the file-system web access server (ADR-003) will reference
IdP-managed identifiers, we need to be explicit about what this management
layer owns versus what it explicitly does not own.

## Decision

**Scope: all management functions for the IdP itself.**
This service/app is the administrative interface for everything the IdP
owns directly:
- Relying party (client) registration and configuration
- User account management
- Device/subkey lifecycle: adding, viewing, and revoking a user's
  master-derived subkeys per client
- Any other IdP-internal configuration (scopes it exposes, signing key
  rotation, etc.)

**Explicit non-scope: file-system ACLs.**
Per-folder access-control and ownership data live entirely on the
file-system side (see ADR-003). The file system may use any identifier
scheme it chooses for "owner" fields; the IdP does not manage or expose a
folder-permission model. The IdP's role in that flow is limited to owning
and asserting the public keys/subject identifiers that the file-system
side then maps to permissions on its own.

This separation keeps the IdP a generic, reusable identity service (usable
by relying parties other than the DFS web access server in the future)
rather than one entangled with a single consumer's authorization model.

## Consequences

- Any future consumer of the IdP (not just the DFS web access server) can
  integrate without inheriting file-system-specific concepts.
- Folder-permission changes never require a round trip through the IdP;
  they're purely a file-system-side operation, keyed on whatever
  identifier the file system chose to store.
- Device/subkey revocation in this app must propagate promptly to the
  OIDC token issuance path (ADR-001), since a revoked device should not
  be able to complete new signature challenges.
- Because ACL ownership is out of scope here, there is no single admin
  screen showing "what can this user access across the whole system" —
  that view, if wanted later, would need to be built as a separate
  aggregation over the file-system side's data, not as a feature of this
  app.

## Alternatives Considered
- **Fold ACL management into this app**: rejected to avoid coupling a
  general-purpose IdP admin tool to one specific consumer's permission
  model.
