# Remove At-Rest Encryption Plan

## Decision

Remove `EncryptedStorage` and per-scope vault keys. Storage remains scoped by
`(subject, application ID)` and is synchronized through the existing approved-
device transport. This is a breaking change: existing encrypted vaults will
not be readable by the new runtime.

## Scope

- Remove `EncryptedStorage` and `EncryptedStorageError`.
- Remove the `crypto` feature and its `chacha20poly1305` and `getrandom`
dependencies from `file-system`.
- Remove vault-key lookup, creation, and OS-keychain storage from the scoped
  filesystem runtime.
- Change scoped filesystems from `FileSystem<EncryptedStorage<NativeStorage>,
  _, _>` to `FileSystem<NativeStorage, _, _>`.
- Update Tauri transport type signatures to use the new scoped filesystem
  alias.
- Delete encryption-specific unit tests and add coverage that scoped native
  storage persists and synchronizes without a keyring.
- Remove encryption and key-transfer claims from documentation, configuration,
  examples, and error messages.

## Steps

1. Update `crates/lidp-service/src/scoped_file_system.rs` to construct
   `FileSystem` directly from `NativeStorage`; delete `RawKeyringRepo`,
   `VAULT_KEY_NAME`, and `load_or_create_key` from this runtime.
2. Update `apps/lidp/src-tauri/src/scoped_transport.rs` to accept the
   `ScopedFileSystem` alias instead of explicitly naming the encrypted storage
   type.
3. Delete `crates/file-system/src/encrypted_storage.rs` and its public export
   from `crates/file-system/src/lib.rs`.
4. Remove the `crypto` feature and unused crypto dependencies from
   `crates/file-system/Cargo.toml`; regenerate `Cargo.lock`.
5. Search the workspace for `EncryptedStorage`, `EncryptedStorageError`,
   `vault-key`, `vault key`, `key transfer`, and `encrypted` and remove stale
   API uses and claims.
6. Decide the migration policy explicitly:
   - simplest: reject existing encrypted vaults and require users to clear or
     restore their local storage;
   - optional one-release migration: retain a separate migration binary that
     reads encrypted vaults with the old keychain entry and rewrites them as
     plaintext. Do not retain encryption code in the runtime.
7. Run `cargo fmt`, `cargo test --workspace`, and
   `cargo hack test --feature-powerset --all-targets`. Test a two-device
   concurrent Automerge update and reconnect after the breaking change.

## Risks

- Existing encrypted vault contents become inaccessible without migration.
- Plaintext files are readable to processes and users with access to the local
  storage directory.
- The device keychain remains necessary for device endpoint identities and
  unrelated OAuth/private-key material; only vault-key usage is removed.

## Acceptance Criteria

- No production reference to `EncryptedStorage`, `EncryptedStorageError`, or
  `vault-key` remains.
- Opening a scope does not access the keyring for storage encryption.
- Approved devices converge filesystem and Automerge writes after reconnecting.
- README and public API do not claim encrypted filesystem storage or vault-key
  transfer.
