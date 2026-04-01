# Parametrized Tests

## rstest Basics

```rust
#[rstest]
#[case::one_member(1)]
#[case::five_members(5)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_group_sizes(#[case] member_count: usize) {
    tester!(alix);
    // ... test with member_count
}
```

Named cases (`#[case::name(value)]`) show in test output as `test_group_sizes::case_1_one_member`.

## Attribute Stacking Order

Macros expand from bottom to top, so the test macro must see the rstest-generated test cases. Use this order:

```rust
#[xmtp_common::timeout(Duration::from_secs(15))]  // 1. Timeout (outermost)
#[rstest::rstest]                                    // 2. Parametrization
#[xmtp_common::test]                                // 3. Test macro
#[cfg_attr(target_arch = "wasm32", ignore)]          // 4. Platform skip
async fn test_streaming() { }
```

## Timeout

```rust
use xmtp_common::time::Duration;

#[xmtp_common::timeout(Duration::from_secs(15))]
#[xmtp_common::test]
async fn test_with_timeout() {
    // Fails if not complete within 15 seconds
}
```

**Source:** `crates/xmtp_mls/src/utils/test/fixtures.rs`, `crates/xmtp_mls/src/groups/tests/test_proposals.rs`
