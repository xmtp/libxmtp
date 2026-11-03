# xmtp_mls_identity

Identity creation and registration (`identity_lifecycle`), identity updates
and their resolution, inbox handles, and key package maintenance. Depends on
`xmtp_mls_core`; names no engine or facade type. Native and WASM. `xmtp_mls`
re-exports its modules at their old paths.

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just test crate xmtp_mls_identity'
```

- `test-hooks` forwards `xmtp_mls_core/test-hooks`; enable it only from
  tests. `xmtp_mls` forwards `test-utils` and `test-hooks`.
- The `identity` test target builds clients through the `xmtp_mls`
  dev-dependency. Tests that read private state stay unit tests.
