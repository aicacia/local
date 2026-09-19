# IdP DB Enrollment TODO

## Goal

Enroll a device and synchronize the IdP database without filesystem snapshots
or late-bound router services.

## Dependencies

- `DB_REPOSITORIES_TODO.md`
- `DB_SYNC_TRANSPORT_TODO.md`

## Plan

- [ ] Bootstrap creates the baseline and the first approved device directly in
  the DB.
- [ ] Pairing authenticates the joining device with the existing device tunnel.
- [ ] The approver writes the device approval record in the DB.
- [ ] Sync the DB to the joining device after approval.
- [ ] Start the router after its concrete DB-backed services are constructed.
- [ ] Replace executor slots with concrete values and native async trait APIs.
- [ ] Keep denial behavior when the local DB cannot safely authorize a request.

## Verification

- [ ] First-device bootstrap succeeds.
- [ ] Device B enrolls through device A and receives IdP state.
- [ ] Restarting either device opens the same local DB state.
- [ ] Revoking B synchronizes and denies B tunnel access.
- [ ] Setup routes compile without `Pin<Box<dyn Future>>` or `dyn` executor
  slots.
