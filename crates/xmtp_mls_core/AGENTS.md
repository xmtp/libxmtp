# xmtp_mls_core

The client context (`LocalContext`), the local identity, the MLS store, the
worker runner and its tasks, events, policies and preflight. It names no
identity-lifecycle, engine or facade type. Native and WASM. `xmtp_mls`
re-exports it as `xmtp_mls::core` and keeps the old `context`, `identity`,
`mls_store` and `worker` paths.

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just test crate xmtp_mls_core'
```

- `test-hooks` items change runtime behavior; enable them only from tests.
  `xmtp_mls` forwards `test-utils` and `test-hooks`.
- Fields that exist only under this crate's cfg live in `HookState`, so a
  crate whose features differ can still build a `LocalContext` with
  `Default`.
- The `mls_core` test target builds clients through the `xmtp_mls`
  dev-dependency. Tests that read private state stay unit tests.
