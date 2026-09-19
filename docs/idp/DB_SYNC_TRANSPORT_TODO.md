# IdP DB Sync Transport TODO

## Goal

Synchronize one `db::Engine` over one existing authenticated bidirectional
Iroh stream.

## Independent scope

This does not define IdP schema or repositories. It may use a small test
schema and in-memory engines.

- [ ] Implement `db::SyncTransport` for the Iroh stream framing API.
- [ ] Reuse existing tunnel authorization before creating the transport.
- [ ] Set a maximum frame size before allocation.
- [ ] Map closed streams and malformed frames to safe sync errors.
- [ ] Run one `db::synchronize` session per connected peer.
- [ ] Trigger another session after reconnect or a local committed write.
- [ ] Keep scheduling outside `db`.

## Verification

- [ ] Two durable engines converge through an Iroh stream.
- [ ] An unauthorized peer cannot begin a sync session.
- [ ] An oversized frame is rejected safely.
- [ ] Disconnecting during sync leaves committed local state valid.
- [ ] Reconnecting transfers offline writes.
