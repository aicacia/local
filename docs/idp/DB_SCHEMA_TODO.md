# IdP DB Schema TODO

## Goal

Define the native `converge` schema that replaces LibSQL tables for replicated
IdP state.

## Independent scope

This defines data shape only. Repository behavior, synchronization transport,
and enrollment handlers are separate work.

- [x] Use UUID primary keys for every table.
- [x] Define tables for users, emails, phone numbers, credentials, keys,
      applications, clients, devices, roles, permissions, grants, OAuth codes,
      consents, and refresh-token state.
- [x] Define unique indexes for client IDs, device public keys, user handles,
      and other externally unique identifiers.
- [x] Define immutable or one-way state for revocations and consumed OAuth
      artifacts.
- [x] Replace LibSQL migrations with idempotent DB schema setup.
- [x] Keep storage namespace ACLs out of this schema.

## Verification

- [x] A new DB initializes every table and index.
- [x] Repeating setup is safe.
- [x] Local duplicate unique values fail.
- [x] Every table has exactly one UUID primary key.
