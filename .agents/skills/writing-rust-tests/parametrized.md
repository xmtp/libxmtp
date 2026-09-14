# Parametrized tests

## rstest cases

```rust
#[rstest]
#[case::one_member(1)]
#[case::five_members(5)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_group_sizes(#[case] member_count: usize) {
    tester!(alix);
    // ...
}
```

Named cases show in output as `test_group_sizes::case_1_one_member`.

## rstest fixtures

`crates/xmtp_mls/src/utils/test/fixtures.rs` defines async fixtures `alix`,
`bo`, `bola`, `caro`, `eve` (`ClientTester`) and `xmtp_client`
(`FullXmtpClient`). Async fixtures need `#[future]` on the parameter and
`#[awt]` on the function.

```rust
// crates/xmtp_mls/src/subscriptions/stream_conversations.rs
#[rstest]
#[case::five_dms(5)]
#[xmtp_common::test]
#[awt]
async fn test_many_concurrent_dm_invites(#[future] alix: ClientTester, #[case] dms: usize) {
    // ...
}
```

Prefer `tester!` inside the body when you need options; use fixtures for the
plain default clients.

## Attribute order

Macros expand bottom to top, so the test macro must see the rstest-generated
cases:

```rust
#[xmtp_common::timeout(Duration::from_secs(15))]   // 1. timeout, outermost
#[rstest::rstest]                                   // 2. parametrization
#[xmtp_common::test]                                // 3. test macro
#[cfg_attr(target_arch = "wasm32", ignore)]         // 4. platform skip
async fn test_streaming() {}
```

## Timeout

```rust
use xmtp_common::time::Duration;

#[xmtp_common::timeout(Duration::from_secs(15))]
#[xmtp_common::test]
async fn test_with_timeout() {
    // panics if not complete within 15 seconds
}
```

Tests only. In production use `xmtp_common::time::timeout` and handle `Expired`.

**Source:** `crates/xmtp_mls/src/utils/test/fixtures.rs`, `crates/xmtp_macro/src/timeout_macro.rs`
