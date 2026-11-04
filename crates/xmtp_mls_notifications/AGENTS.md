# xmtp_mls_notifications

Push notifications: the client registration and settings (`client`) and
the durable task that keeps the backend registration current (`worker`).
Depends on `xmtp_mls_core` and `xmtp_mls_engine`; names no facade,
streams or device-sync type. Native and WASM. `xmtp_mls` re-exports
`client` as `client::notifications`.

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just test crate xmtp_mls_notifications'
```

- Unit tests run on the doc-hidden `client::test_support` peers, which
  build on `EngineTestPeer` and script only the notification requests.
- Client build, restart and task-runner contracts stay on a real client in
  `crates/xmtp_mls/src/client/tests/notifications.rs`.
- `test-hooks` changes runtime behavior; enable it only from tests.
  `xmtp_mls` forwards `test-utils` and `test-hooks`.
