# xmtp_mls_streams

Message and conversation streams over the engine receiver: local delivery
and message readers, catch-up, stream failures, stream statistics and the
native callback adapters (`router_callbacks`). Depends on `xmtp_mls_core`,
`xmtp_mls_engine`, `xmtp_mls_messages` and `xmtp_mls_server_config`;
names no facade, device-sync or notifications type. Native and WASM.
`xmtp_mls` re-exports it, with the engine's `barrier` and `incoming`
modules, as `subscriptions`.

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just test crate xmtp_mls_streams'
```

- Unit tests in `src/` that read private stream state build peers through
  `xmtp_mls_engine::groups::test_support::EngineTestPeer`, a real-backend
  peer without workers.
- The `streams` target builds real clients through the `xmtp_mls`
  dev-dependency, which also enables this crate's `test-utils` and
  `test-hooks`, so the default command builds it. Stream contracts of the
  client itself stay in `crates/xmtp_mls/src/client/tests/`.
- Reproduce `bidi_fuzz` failures with the logged `XMTP_BIDI_FUZZ_SEED`,
  default rounds, and `--retries 0`.
- `test-hooks` changes runtime behavior; enable it only from tests.
  `xmtp_mls` forwards `test-utils` and `test-hooks`.
