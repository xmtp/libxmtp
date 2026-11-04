# xmtp_mls_engine

Groups with their intents, sync, commit log, welcomes and app data, and the
stream receiver that feeds them (`subscriptions`: barrier, incoming,
recovery and connection state). Depends on `xmtp_mls_core`,
`xmtp_mls_identity`, `xmtp_mls_messages` and `xmtp_mls_server_config`;
names no facade, streams, device-sync or notifications type. Native and
WASM. `xmtp_mls` re-exports `groups` and the receiver modules at their old
paths.

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just test crate xmtp_mls_engine'
```

- Unit tests build peers through `groups::test_support::EngineTestPeer`, a
  real-backend peer without workers. Leaves that need the facade client,
  streams, device sync or notifications live in
  `crates/xmtp_mls/src/client/tests/engine_suites/`.
- `test-utils` exposes the doc-hidden `groups::test_support` items that
  facade tests call. `test-hooks` forwards the core, identity and server
  config hooks; enable it only from tests. `diagnostics` gates the
  read-only group snapshots of apps/chaos. `xmtp_mls` forwards all three.
