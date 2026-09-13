# ADR 0001: Global Identity Is Authoritative

## Status

Accepted

## Context

A joined installation must synchronize identity and management state before it can be trusted. The existing LibSQL runtime is local-only. A database snapshot bridge is unsafe because the filesystem has independent record writes and current filesystem repositories are not lossless projections of every LibSQL row.

## Decision

The Global Identity Namespace is the authoritative replicated source for identity and management state.

- It uses the fixed `VaultId::global_identity()` and is always Full.
- Every replicated record has a canonical, lossless encoding at a stable path.
- A writer stages a complete revision under `revisions/<revision>/` and writes a hash-bound manifest only after every record is durable.
- Receivers validate every manifest entry before atomically activating the revision locally.
- A local SQL database, if retained, is a replaceable read cache materialized only from an activated manifest. It is never an authority or a sync source.
- Private key material remains device-local in the OS keyring and is not part of a global revision.
- A Join bootstrap grant is single-use and binds the global vault, both endpoint IDs, a nonce, and an expiry. Installation Setup advances only after the activated revision contains the joining device's approved record.

## Consequences

- Identity writes must publish a new complete global revision rather than independently mutating a local database.
- Existing filesystem repositories and LibSQL repositories must be replaced or migrated where their encodings omit state.
- Initial implementation can materialize a validated revision into a local SQL cache, but all updates must originate from the global revision writer.
