# Existing Test Index

This index maps source tests to the [requirement catalogue](existing-requirements.md). It started at `cc878025dab2` on 2026-09-01. Task 14 removes deleted tests, records moved tests, and adds notable Phase 3 coverage from base `4d651186f`.

The detailed index is split by area. This makes the files easier to scan and keeps each review task bounded.

## Phase 2 backend tests

The backend suite and its owners are listed in the [Phase 2 ownership map](existing-requirements.md#phase-2-backend-ownership).
Run it with `just backend db-up`, `just backend sql-check`, and `just backend test`.
It replaces the Phase 1 greeting test, not the client SDK suites.

## Counting model

- One row represents one source test declaration, macro template, executable or ignored documentation test, or explicit manual scenario.
- Parameter cases, loops, target matrices, and browser matrices stay in one row. Their cases are in the form, gates, or cases column.
- Source rows are not runner counts. Use `dev/nix-shell 'cargo nextest list --profile ci'` for the current native test cases. The Phase 0 expanded count is historical.
- Removed orphan test modules no longer have catalogue rows. A surviving requirement without a direct test anchor is listed below as a gap.
- Helpers, fixtures, runners, benchmarks, commented-out tests, data generators, and report-only scripts are not rows.

Skipped, ignored, feature-gated, target-gated, service-dependent, time-sensitive, and manual tests remain in the index. Each area file states these conditions.

## Inventory summary

The requirement count for an area is the number of distinct IDs that its tests use. Shared IDs can occur in more than one area. Counts below describe the current catalogue rows, including declared skipped tests. The backend ownership map is separate.

| Area | Test entries | Distinct requirement IDs |
| --- | ---: | ---: |
| [`xmtp_mls` group integration tests](existing-tests/xmtp-mls-groups.md) | 280 | 192 |
| [`xmtp_mls` group implementation and messages](existing-tests/xmtp-mls-inline-groups.md) | 232 | 75 |
| [`xmtp_mls` client, identity, subscriptions, and workers](existing-tests/xmtp-mls-client-workers.md) | 225 | 134 |
| [`xmtp_mls_common`](existing-tests/xmtp-mls-common.md) | 271 | 29 |
| [Database, identity, cryptography, and archive crates](existing-tests/core-crates.md) | 268 | 128 |
| [Mobile, Node, and WebAssembly bindings](existing-tests/bindings.md) | 374 | 111 |
| [API crates](existing-tests/api.md) | 169 | 75 |
| [Other Rust crates and applications](existing-tests/rust-apps-support.md) | 253 | 69 |
| [JavaScript Agent SDK](existing-tests/agent-sdk.md) | 117 | 29 |
| [Release tools](existing-tests/release-tools.md) | 179 | 28 |
| [Manual test scenarios](existing-tests/manual-scenarios.md) | 3 | 3 |
| [Browser and Node JavaScript SDKs](existing-tests/javascript-sdks.md) | 451 | 123 |
| [Android SDK and example](existing-tests/android.md) | 226 | 112 |
| [iOS SDK](existing-tests/ios.md) | 216 | 127 |
| **Total** | **3,264** | **1,080 referenced IDs** |

## Review records

- [Retired requirement ID crosswalk](retired-requirement-ids.md)

## Phase 3 coverage gaps

The source check found four live IDs without a current direct catalogue test.
These IDs stay live because the subject was not proved obsolete. Do not treat
this list as test coverage. Restoring tests is outside Task 14's consolidation scope.

| ID | Gap |
| --- | --- |
| `GINLINE-REQ-082` | The welcome-pointer extension serialization test was removed. Current key-package extension tests need an assertion comparison before they can replace it. |
| `MLS-REQ-064` | The orphan history worker's database reconnect test was removed. General reconnect tests do not prove its manual sync-group change assertion. |
| `MLS-REQ-066` | The orphan external sync-group handle test was removed. Group membership tests do not directly prove this particular handle path. |
| `MLS-REQ-076` | The orphan consent metric and source-event-isolation test was removed. Active message-based consent tests stay, but they do not prove that exact old metric sequence. |

IOS-REQ-136 and IOS-REQ-137 still declare their tests and still skip before setup.
The Phase 3 plan's DELETE disposition is corrected to KEEP for these two IDs.

P3-TST-002 permits bounded RESOURCE_EXHAUSTED retry at the size floor. The status
also represents rate limits and stream token buckets, so it does not identify one
remedy. INVALID_ARGUMENT and OUT_OF_RANGE are terminal at that floor. The HTTP/2
cap test asserts queue-then-admit because API-132 specifies no backend rejection.
