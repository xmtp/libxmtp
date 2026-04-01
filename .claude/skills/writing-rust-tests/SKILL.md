---
name: writing-rust-tests
description: Use when writing, modifying, or reviewing Rust tests in crates/ or bindings/ - covers test macros, fixtures, assertions, WASM compatibility, and how to run tests
---

# Writing Rust Tests in libxmtp

## Essentials

Always use `#[xmtp_common::test]` instead of `#[test]` or `#[tokio::test]` in crates/. It handles native/WASM dispatch and logging init.

```rust
#[xmtp_common::test(unwrap_try = true)]
async fn test_something() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    group.send_msg(b"hello").await;  // internally unwraps; returns ()

    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let msg = bo_group.test_last_message_bytes().await?;
    assert_eq!(msg.unwrap(), b"hello");
}
```

- `unwrap_try = true` transforms `?` into `.unwrap()` — use for tests returning `()`
- `tester!(name)` creates a registered test client with logging
- `group.invite(&other).await?` and `group.send_msg(b"msg").await` from `MlsGroupExt` trait

**Exception:** `bindings/mobile` tests use `#[tokio::test(flavor = "multi_thread")]` directly (never targets WASM).

## Running Tests

All `just test` and `just wasm test` variants pass extra args through to `cargo nextest run`:

```bash
just test                           # All (v3 + d14n)
just test v3                        # V3 only
just test v3 test_send_message      # Specific test in v3
just test crate xmtp_mls            # Single crate
just wasm test                      # All WASM (v3 + d14n)
```

Tests that create clients need the backend: `just backend up` / `just backend down`

See [Running Tests](running.md) for nextest filter syntax, profiles, WASM/d14n targeting, and CI commands.

## Writing Philosophy

- **Test behavior, not implementation.** Don't assert on internal state that could change.
- **One assertion focus per test.** `test_send_message` shouldn't also verify group creation.
- **Don't test the obvious.** Skip trivial getters, derive impls, framework behavior.
- **Don't duplicate coverage.** If `test_talk_in_dm` covers DM messaging, don't add `test_dm_send_message`.
- **Use `tester!` and convenience methods.** Don't manually build clients.
- **Prefer `unwrap_try = true`.** Eliminates noisy `.unwrap()` chains.

## Further Reference

- [Fixtures & tester! Macro](fixtures.md) — tester! options, chaining, convenience methods, TesterBuilder
- [WASM Compatibility](wasm.md) — cfg_attr, if_native!/if_wasm!, conditional compilation macros
- [Assertions & Waiting](assertions.md) — assert_ok!, assert_err!, wait_for_eq/ok/some/ge, custom assertion patterns
- [Parametrized Tests](parametrized.md) — rstest, #[case], timeouts, attribute stacking
- [Test Utilities](utilities.md) — Data generators, logging, retry, TestLogReplace, toxiproxy
- [Running Tests](running.md) — Full nextest filter syntax, profiles, env vars, CI, backend services
