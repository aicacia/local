# IdP DB Conflict Policy TODO

## Goal

Define fail-closed behavior for concurrent replicated IdP changes.

## Independent scope

This policy can be specified and tested with an in-memory DB before the
production repositories or Iroh adapter exist.

- [x] Classify security-sensitive rows.
- [x] Deny authentication when credentials or user keys conflict.
- [x] Deny token issuance when client, redirect URI, consent, role,
      permission, or grant state conflicts.
- [x] Deny tunnel access when device approval or revocation conflicts.
- [x] Keep authorization codes and refresh tokens consumed after any merge.
- [x] Require explicit administrative resolution before conflicted state is
      usable.
- [x] Record each explicit resolution in `idp_conflict_resolutions`.

`idp_model::replica` exposes the guards. Repositories must pass every row
participating in a security decision to its matching guard before use.

## Verification

- [x] Conflicted device state denies tunnel authorization.
- [x] Conflicted client state denies OAuth token issuance.
- [x] Conflicted role or permission state denies authorization.
- [x] Conflicted credentials or signing keys deny authentication.
- [x] A consumed code or refresh token cannot become usable after sync.
- [x] Explicit resolution enables only the selected valid state.
