# xmtp_backend_config

Owns config types, defaults, environment references, and pure validation.
Wire/crypto/I/O checks and live objects belong in `apps/backend`; runtime facts
passed as arguments may be validated here. See [CONF](../../docs/specs/CONF-backend-configuration.md).

```sh
dev/nix-shell 'just test crate xmtp_backend_config'
dev/nix-shell 'just backend schema' # commit docs/schemas/backend-v1.json too
```

- Keep normal dependencies light: no client, protocol, crypto, network, database,
  runtime, or workspace-hack crates. Subsystems depend on this crate, never the
  reverse. `xmtp_common` is a dev-dependency for tests only.
- Config doc comments become schema descriptions; regenerate after edits.
  Preserve schema key order (`schemars/preserve_order`).
- Refusals name keys, never values. Release example fixtures are immutable.
- Environment-mutating tests use `tests::child_test` and `assert_one_test_passed`.
