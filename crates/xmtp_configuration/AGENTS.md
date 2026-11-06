# xmtp_configuration

Cross-crate constants and deployment configuration; private constants stay local.
Run `dev/nix-shell 'just test crate xmtp_configuration'`.

- Use `common/<area>.rs` for shared values; differing test/prod values must export
  the same names from both halves. Never copy a shared value locally.
- Worktree-dependent values are environment-reading functions, not constants.
  WASM uses `option_env!`, so addresses are fixed at build time.
- Keep this a leaf: normal dependencies are `semver` and `thiserror`. Values
  needing another crate's types belong there. Use `src/ns.rs` for nanoseconds.
- `common/server.rs` owns deployment snapshots/providers; fields fall back to
  `BACKEND_DEFAULT_*`. Contract: [CONF](../../docs/specs/CONF-backend-configuration.md).
