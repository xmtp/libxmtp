---
name: writing-rust-tests
description: Use when writing, modifying, or reviewing Rust tests in crates/, apps/backend, or bindings/ - covers the test macro and its options, where tests live, tester! fixtures, the ephemeral backend, assertions and wait helpers, WASM compatibility, and how to run tests
---

# Writing Rust Tests in libxmtp

## Essentials

Always `#[xmtp_common::test]`, never `#[test]` or `#[tokio::test]`. It picks
`tokio::test` on native and `wasm_bindgen_test` on wasm, and installs logging.
The only exception is a crate `xmtp_common` depends on, such as `xmtp_logging`.

```rust
#[xmtp_common::test(unwrap_try = true)]
async fn test_something() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    group.send_msg(b"hello").await;   // MlsGroupExt; unwraps internally

    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let msg = bo_group.test_last_message_bytes().await?;
    assert_eq!(msg.unwrap(), b"hello");
}
```

Macro options (`crates/xmtp_macro/src/test_macro.rs`); an unknown key is a
compile error:

| Option | Default | Use |
| --- | --- | --- |
| `unwrap_try = true` | off | `?` becomes `.unwrap()` in a test returning `()` |
| `flavor = "multi_thread"` | `current_thread` | tests that need parallel tokio workers |
| `worker_threads = 4` | CPU count | with `multi_thread` only |
| `disable_logging = true` | off | quiet one test; `XMTP_TEST_LOGGING=false` or `CI=true` quiets all |

`tester!(name)` builds a registered client whose name appears in logs. The
macro and every helper here sit behind `xmtp_common`'s `test-utils` feature or
`cfg(test)`.

## Where tests live

- Beside the module they exercise. A short file keeps `#[cfg(test)] mod tests`
  inline. A small suite gets a module-local `tests.rs`. A large suite gets a
  `tests/` directory split by behaviour (`crates/xmtp_mls/src/groups/tests/`).
  No crate-wide `tests/` directory of unrelated modules.
- One test-support module per crate: `crates/xmtp_mls/src/utils/test/`,
  `apps/backend/src/test_support.rs`.
- `#[cfg(test)]` for helpers only this crate uses;
  `#[cfg(any(test, feature = "test-utils"))]` when another crate imports them,
  and forward the feature to dependencies.
- Every new API endpoint gets an integration test: happy path, each error,
  each limit. Integration tests cross real service boundaries.
- Moving a test preserves its assertions, coverage, and runner inclusion.
- Never disable a test to hide a bug.

## Running

```bash
just test                              # workspace, nextest profile ci
just test workspace test_send_message  # one test by name
just test crate xmtp_mls xmtp_db       # one or more crates
just wasm test                         # configured wasm crates
just backend test --lib config         # apps/backend module; cargo test, needs `just backend db-up`
```

Client tests need the stack: `just backend up`. Ports differ per worktree; see
the `working-with-worktrees` skill. Filters and profiles: [running.md](running.md).

## Philosophy

- Test behaviour, not implementation. Skip trivial getters and derive impls.
- One focus per test. Do not duplicate coverage an existing test gives.
- Use `tester!` and the convenience methods. Never build a client by hand.
- Use the wait helpers. Never a bare `sleep` loop.

## Reference

- [fixtures.md](fixtures.md) — `tester!` options, convenience methods, `MlsGroupExt`, mobile fixtures
- [backend.md](backend.md) — `apps/backend` fixtures, `EphemeralBackend` for `xmtp_mls` tests
- [assertions.md](assertions.md) — `assert_ok!`, `assert_err!`, `wait_for_*`, notify helpers
- [utilities.md](utilities.md) — generators, logging control, `LogCapture`, retry, toxiproxy
- [parametrized.md](parametrized.md) — rstest cases and fixtures, timeouts, attribute order
- [wasm.md](wasm.md) — skipping on wasm, platform macros, cross-platform time
- [running.md](running.md) — nextest filters, profiles, env vars, CI, coverage
