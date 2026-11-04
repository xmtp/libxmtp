# xmtp_mls_device_sync

Device sync: the sync worker and its sync group, preference sync, and
archive export and import. Depends on `xmtp_mls_core`, `xmtp_mls_engine`
and `xmtp_mls_identity`; names no facade, streams or notifications type.
Native and WASM. `xmtp_mls` re-exports it as `worker::device_sync`.

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just test crate xmtp_mls_device_sync'
```

- Unit tests in `src/` build peers through
  `xmtp_mls_engine::groups::test_support::EngineTestPeer`, a real-backend
  peer without workers.
- The `device_sync` target builds real clients through the `xmtp_mls`
  dev-dependency, which also enables this crate's `test-utils` and
  `test-hooks`, so the default command builds it. It reaches internals
  through the doc-hidden `test_support` module; archive fixtures live in
  `tests/assets`. Task-handler contracts stay in
  `crates/xmtp_mls/src/client/tests/device_sync.rs`.
- `test-hooks` changes runtime behavior; enable it only from tests.
  `xmtp_mls` forwards `test-utils` and `test-hooks`.
