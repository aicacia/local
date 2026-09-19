# IdP DB Conflict Policy TODO

## Goal

Define fail-closed behavior for concurrent replicated IdP changes.

## Independent scope

This policy can be specified and tested with an in-memory DB before the
production repositories or Iroh adapter exist.

- [ ] Classify security-sensitive fields and rows.
- [ ] Deny authentication when credentials or user keys conflict.
- [ ] Deny token issuance when client, redirect URI, consent, role,
  permission, or grant state conflicts.
- [ ] Deny tunnel access when device approval or revocation conflicts.
- [ ] Keep authorization codes and refresh tokens consumed after any merge.
- [ ] Require explicit administrative resolution before conflicted state is
  usable.
- [ ] Define audit records for each explicit resolution.

## Verification

- [ ] Conflicted device state denies tunnel authorization.
- [ ] Conflicted client state denies OAuth token issuance.
- [ ] Conflicted role or permission state denies authorization.
- [ ] Conflicted credentials or signing keys deny authentication.
- [ ] A consumed code or refresh token cannot become usable after sync.
- [ ] Explicit resolution enables only the selected valid state.
