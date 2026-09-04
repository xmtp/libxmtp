# Existing Test Index

This index maps each repository-owned test to one or more entries in the [requirement catalogue](existing-requirements.md). It describes the `libxmtp` source tree at commit `cc878025dab2` on 2026-09-01.

The detailed index is split by area. This makes the files easier to scan and keeps each review task bounded.

## Counting model

- One row represents one source test declaration, macro template, executable or ignored documentation test, or explicit manual scenario.
- Parameter cases, loops, target matrices, and browser matrices stay in one row. Their cases are in the form, gates, or cases column.
- The index has 3,377 source test entries and 3 manual scenarios, for 3,380 rows.
- The Rust source inventory includes 45 Cargo-listed doctests. The default native `cargo test --workspace --all-targets -- --list` command expands Rust parameter and macro cases to 2,621 runnable names. This expanded count does not replace the source-level rows.
- Nine indexed Rust declarations are in orphan files that are not linked into the `xmtp_mls` module tree. They remain visible because they are repository-owned test source.
- Helpers, fixtures, runners, benchmarks, commented-out tests, data generators, and report-only scripts are not rows.

Skipped, ignored, feature-gated, target-gated, service-dependent, time-sensitive, and manual tests remain in the index. Each area file states these conditions.

## Inventory summary

The requirement count for an area is the number of distinct IDs that its tests use. Shared IDs can occur in more than one area. The catalogue has 969 unique requirement definitions.

| Area | Test entries | Distinct requirement IDs |
| --- | ---: | ---: |
| [`xmtp_mls` group integration tests](existing-tests/xmtp-mls-groups.md) | 279 | 188 |
| [`xmtp_mls` group implementation and messages](existing-tests/xmtp-mls-inline-groups.md) | 221 | 61 |
| [`xmtp_mls` client, identity, subscriptions, and workers](existing-tests/xmtp-mls-client-workers.md) | 221 | 107 |
| [`xmtp_mls_common`](existing-tests/xmtp-mls-common.md) | 271 | 29 |
| [Database, identity, cryptography, and archive crates](existing-tests/core-crates.md) | 277 | 100 |
| [Mobile, Node, and WebAssembly bindings](existing-tests/bindings.md) | 364 | 100 |
| [API crates](existing-tests/api.md) | 274 | 73 |
| [Other Rust crates and applications](existing-tests/rust-apps-support.md) | 318 | 87 |
| [JavaScript Agent SDK](existing-tests/agent-sdk.md) | 118 | 28 |
| [Release tools](existing-tests/release-tools.md) | 188 | 29 |
| [Manual test scenarios](existing-tests/manual-scenarios.md) | 3 | 3 |
| [Browser and Node JavaScript SDKs](existing-tests/javascript-sdks.md) | 423 | 91 |
| [Android SDK and example](existing-tests/android.md) | 216 | 104 |
| [iOS SDK](existing-tests/ios.md) | 207 | 119 |
| **Total** | **3,380** | **969 unique definitions** |

## Review records

- [Adversarial review and reduction notes](adversarial-review.md)
- [Retired requirement ID crosswalk](retired-requirement-ids.md)
