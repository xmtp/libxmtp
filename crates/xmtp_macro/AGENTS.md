# xmtp_macro

Proc macros. `#[xmtp_common::test]`, builders, error codes, spans.

## Commands

```bash
just check crate xmtp_macro
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_macro
```

## Gotchas

- No unit tests. Doc examples only.
- A change rebuilds every dependent crate.
