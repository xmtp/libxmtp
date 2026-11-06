# xmtp_macro

Use `#[test]` to avoid a dependency cycle with `xmtp_common`.
Run `dev/nix-shell 'just test crate xmtp_macro'`.

- Token tests live in `src/*_test.rs`; trybuild fixtures in `tests/ui`.
  Refresh expected diagnostics with `TRYBUILD=overwrite`.
- Change SDK metadata markers in both macro and generator. The
  [generator guide](../../apps/xmtp_sdk_bindgen/README.md) owns their format.
  Keep façade marker checks; handwritten marker duplication is rejected.
