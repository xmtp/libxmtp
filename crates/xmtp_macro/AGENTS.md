# xmtp_macro

Proc macros. `#[xmtp_common::test]`, error codes, spans.

## Commands

```bash
just check crate xmtp_macro
just test crate xmtp_macro
```

## Gotchas

- `src/sdk_export_test.rs` and `src/sdk_member_test.rs` hold token-stream
  tests; `tests/sdk_export.rs` runs the trybuild fixtures in `tests/ui`.
  Refresh a fixture's expected error with `TRYBUILD=overwrite`.
- `src/facade_markers_test.rs` reads `crates/xmtp_sdk/src` as tokens and
  fails when a façade file spells out a marker that the macro writes, or
  gives a `doc` attribute a value that is not a string literal.
- Tests use `#[test]`: `xmtp_common` depends on this crate, so its test macro
  would be a cycle.
- `sdk_export` writes the `@xmtp-*` markers that `apps/xmtp_sdk_bindgen` reads.
  Change a marker in both places; `apps/xmtp_sdk_bindgen/README.md` lists them.
- A change rebuilds every dependent crate.
