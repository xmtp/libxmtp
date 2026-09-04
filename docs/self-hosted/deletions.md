<!-- markdownlint-configure-file {"MD013": false, "MD001": false} -->
# libxmtp Deletion Inventory

This is an ephemeral Phase 0 document for the self-hosted transition. Date: 2026-09-04.
The deletion plan in Ref (<https://plan.ref.tools/Acpbh4okwkEPsx36>) approved it. Read
`docs/self-hosted/project.md` first. The behavior wiki is in `docs/self-hosted/existing/`.

All paths are repo-relative. Line counts come from `wc -l` on this checkout and are
approximate. Items marked **verify** need a check by the implementer before the change.

## Table Of Contents

1. [Executive Summary](#1-executive-summary)
2. [Structural Facts](#2-structural-facts)
3. [The `xmtp_api_d14n` Crate](#3-the-xmtp_api_d14n-crate)
4. [Ordering Machinery](#4-ordering-machinery)
5. [The `d14n` Cargo Feature](#5-the-d14n-cargo-feature)
6. [Streaming](#6-streaming)
7. [Protos And `xmtp_proto`](#7-protos-and-xmtp_proto)
8. [Payer And Blockchain](#8-payer-and-blockchain)
9. [v3 Transport](#9-v3-transport)
10. [Apps](#10-apps)
11. [Client Config, Bindings, SDKs](#11-client-config-bindings-sdks)
12. [Docker, Dev Scripts, Nix, CI](#12-docker-dev-scripts-nix-ci)
13. [Ordered Deletion Plan](#13-ordered-deletion-plan)
14. [Keep List](#14-keep-list)
15. [Cargo Dependencies](#15-cargo-dependencies)
16. [Unverified Items](#16-unverified-items)

## 1. Executive Summary

About 62,000 lines of Rust are deleted across the project. The repo has 296,757 lines of
Rust in `crates/`, `apps/`, and `bindings/`, of which 68,008 are generated protobuf code.
The deletions are about 21 percent of the total. About 17,000 more lines are replaced or
rewritten rather than removed. About 2,000 lines of YAML, Nix, shell, and Docker config
also go.

Nothing is a separate deletion wave. Every deletion belongs to a project phase:

| Phase | What | Approx lines |
| --- | --- | --- |
| 0 | d14n-only test files (see the Phase 0 test-deletion plan) | 1,000 |
| 0 | `apps/xnet` with its Nix references, workspace entries, and xnet-only dependencies | 8,000 |
| 1 | `proto/` folder authoritative with the old generated tree deleted; `xmtp_proto` `keystore_api` generated files and `nightly-protos.yml` deleted with the proto move; shared crate scaffolded; `apps/backend` scaffold; CI workflows for xdbg, wasm, and the browser SDK disabled (not deleted; `test-xdbg.yml` is called by `test.yml`, and `cross-test.yml` is read by `release-gate-plan.yml`, so deleting them needs edits to those callers first) | 6,600 |
| 1 | `dev/drivers` (`cross_talk_test`, `cross_version_test`, `xdbg_driver_lib`) and the matching nix packages | 1,400 |
| 2 | Move the validation logic and its 8 logic tests into the shared crate, make the backend use it, then delete the `apps/mls_validation_service` shell (main, health, config); `xmtp.mls_validation.v1` protos; its nix package, release workflow, `dev/validation_service`, `dev/build_validation_service*` | 2,500 |
| 3 | Integration: collapse the `d14n` feature; delete the d14n arm; replace the v3 arm; delete the ordering machinery; simplify `refresh_state.rs`; delete the xmtpv4 and migration copies from `proto/`; delete the legacy streaming stack and make XIP-83 bidi the default; delete `registration_visible`; rename the crate; collapse env and URL config; simplify test harness aliases; `grpc.gateway` and `google.api` generated files; retarget xdbg; Docker | 43,000 |

The largest blocks:

| Rank | Area | Approx lines |
| --- | --- | --- |
| 1 | `crates/xmtp_api_d14n` minus its keepers (section 3) | 15,500 |
| 2 | `crates/xmtp_proto/src/gen` packages with no consumer after Phase 3 | 22,600 |
| 3 | `xmtp.mls.api.v1` generated code, replaced by `xmtp.backend.v1` | 9,200 |
| 4 | `apps/xnet` | 7,995 |
| 5 | Legacy streaming stack in `crates/xmtp_mls/src/subscriptions` | 3,900 |
| 6 | Ordering machinery outside `xmtp_api_d14n` | 2,700 |
| 7 | d14n-only tests, `dev/drivers`, validation shell | 2,600 |

## 2. Structural Facts

These facts change how the deletions must run.

### 2.1 `xmtp_api_d14n` is the main API crate

It is not an optional side path. Evidence:

- `crates/xmtp_mls/src/definitions.rs:11` aliases `xmtp_api_d14n::definitions::XmtpApiClient`.
- `crates/xmtp_mls/src/builder.rs:21-24` imports `TrackedStatsClient`, `CursorStore`, and `XmtpQuery` from it.
- All three bindings depend on it: `bindings/mobile/Cargo.toml:29`, `bindings/node/Cargo.toml:36`, `bindings/wasm/Cargo.toml:52`.
- `crates/xmtp_api/Cargo.toml:21` depends on it.
- `crates/xmtp_api_d14n/src/queries/v3/` (1,687 lines) is the v3 client that the default build uses.

Decision: the crate is **renamed**, not deleted. The new name (for example `xmtp_api_backend`)
is decided in the Phase 3 plan. The keepers stay in the renamed crate. The d14n arm, the
v3 arm, and the ordering machinery are deleted from it.

### 2.2 The `d14n` feature is a switch between two live backends

`crates/xmtp_api_d14n/src/definitions.rs:11-17` picks `V3Client` or `D14nClient` with
`if_v3!` and `if_d14n!`. Removing the feature means picking one arm at every `cfg` site
(about 50 sites, section 5), not deleting a block.

### 2.3 The XIP-83 bidi client stays

`crates/xmtp_api_d14n/src/queries/bidi.rs` and `bidi_transport.rs` are the XIP-83 client.
The new backend supports XIP-83 streaming (`docs/self-hosted/project.md`, Phase 2). The bidi
client is kept. It loses the `BidiBinding` type parameter and the d14n instantiation, and it
becomes the default stream path in Phase 3.

### 2.4 Proto types move early

Phase 1 copies every `.proto` still referenced by kept code into the new `proto/` folder,
including the xmtpv4 envelope types that kept code uses today. Phase 3 deletes the xmtpv4
copies after their consumers are simplified. See section 7.

## 3. The `xmtp_api_d14n` Crate

Total: 24,913 lines.

### 3.1 Keep in the renamed crate

| Path | What | Lines | Dependents | Action |
| --- | --- | --- | --- | --- |
| `crates/xmtp_api_d14n/src/queries/bidi.rs` | XIP-83 bidi connection core (HTTP/2 full duplex) | 1,138 | `xmtp_mls` streaming | Keep. Remove the `BidiBinding` type parameter and the d14n instantiation. |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | XIP-83 topic ledger and envelope demux, with its tests | 6,172 | Same | Keep. Same collapse to one binding. |
| `crates/xmtp_api_d14n/src/queries/bidi_transport_props.rs` | proptest model of the ledger | 751 | Tests the kept ledger | Keep. |
| `crates/xmtp_api_d14n/src/queries/boxed_streams.rs` | Generic stream boxing | 259 | Stream consumers | Keep. |
| `crates/xmtp_api_d14n/src/queries/stream/extractor.rs` | Flattens `EnvelopeCollection` items from a `TryStream` | 194 | Stream consumers | Keep. |
| `crates/xmtp_api_d14n/src/middleware/auth.rs` | `AuthCallback`, `AuthHandle`, `Credential`, gateway JWT body | 518 | `bindings/mobile/src/mls/gateway_auth.rs`, `bindings/node/src/client/gateway_auth.rs`, `bindings/wasm/src/client/gateway_auth.rs` | Keep the three type names. Phase 6 replaces the gateway-specific body. |
| `crates/xmtp_api_d14n/src/queries/api_stats.rs` | `TrackedStatsClient`, per-endpoint call counters | 356 | `crates/xmtp_mls/src/definitions.rs` | Keep. Rewrite for the new endpoint set. |
| `crates/xmtp_api_d14n/src/queries/client_bundle.rs` | `ClientBundle` enum (D14n, V3, Migration) and `ClientBundleBuilder` | 322 | `bindings/node/src/client/backend.rs`, `bindings/wasm/src/client/backend.rs` | Keep the builder name. Collapse the enum to one client. |
| `MessageBackendBuilder` (in `crates/xmtp_api_d14n/src/queries/builder.rs`) | Builder type used by the bindings | part | 31 references outside the crate | Keep the name. |
| `crates/xmtp_api_d14n/src/protocol/traits/xmtp_query.rs` | `XmtpQuery` trait and `XmtpEnvelope` | 79 | `crates/xmtp_api/src/xmtp_query.rs`, `crates/xmtp_mls/src/context.rs:23`, tests | Spec 002 decides whether the new client keeps this abstraction. |
| `crates/xmtp_api_d14n/src/test/` | Mock client and test client definitions | 274 | Test harness | Rewrite against the new backend. |

### 3.2 Delete: the d14n arm (Phase 3, step 2)

| Path | What | Lines | Action |
| --- | --- | --- | --- |
| `crates/xmtp_api_d14n/src/endpoints/d14n/` | v4 endpoints: `QueryEnvelopes`, `SubscribeTopics`, `PublishClientEnvelopes`, `GetNodes`, `GetInboxIds`, `GetNewestEnvelopes`, health, cutover | 703 | Delete |
| `crates/xmtp_api_d14n/src/queries/d14n/` | xmtpd client (mls, identity, streams, cutover) | 1,476 | Delete |
| `crates/xmtp_api_d14n/src/queries/combined/` | v3 to d14n `MigrationClient`; its `tests.rs` (340) goes in Phase 0 | 625 | Delete; drop the `MigrationTestClient` alias |
| `crates/xmtp_api_d14n/src/middleware/multi_node_client/` | Gateway `GetNodes` fan-out and health check | 558 | Delete; `MULTI_NODE_TIMEOUT_MS` goes too |
| `crates/xmtp_api_d14n/src/middleware/read_write_client/` | Writes to payer, reads to xmtpd | 218 | Delete |
| `crates/xmtp_api_d14n/src/middleware/readonly_client.rs` | Read-only wrapper; used only by `middleware/mod.rs:7-8` | 157 | Delete unless the new client keeps a read-only mode (**verify**) |
| `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs` | Handles `SubscribeTopicsResponse` variants | 296 | Delete with `SubscribeTopics` |
| `crates/xmtp_configuration/src/common/d14n.rs` | `Originators`, `PAYER_WRITE_FILTER`, `CUTOVER_REFRESH_TIME`, `D14N_MIGRATION_MSG_REGEX` | 25 | Delete the file; remove `mod d14n;` and `pub use d14n::*;` from `crates/xmtp_configuration/src/common.rs:2,10` |
| `crates/xmtp_db/src/encrypted_store/d14n_migration_cutover.rs` | Cutover-time table | 164 | Delete the table and `crates/xmtp_db/migrations/2026-02-04-203357-0000_d14n_migration_cutover` |

### 3.3 Replace: the v3 arm (Phase 3, step 3)

| Path | What | Lines | Action |
| --- | --- | --- | --- |
| `crates/xmtp_api_d14n/src/endpoints/v3/` | v3 endpoint definitions (mls and identity); each maps 1:1 to a node-go RPC | 1,274 | Replace with `xmtp.backend.v1` endpoints |
| `crates/xmtp_api_d14n/src/queries/v3/` | v3 client assembly (connection, mls, identity, streams, bidi binding) | 1,687 | Replace |
| `crates/xmtp_api/src/mls.rs` | High-level MLS API calls; the shape survives, the wire calls change | 786 | Replace |
| `crates/xmtp_api/src/identity.rs` | Same for identity | 339 | Replace |
| `crates/xmtp_proto/src/traits/combinators/v3_paged.rs` | v3 pagination combinator | 304 | Delete |
| `crates/xmtp_api_grpc/src/grpc_client/test/clients.rs` | v3 and toxiproxy test clients | 154 | Replace |

### 3.4 Delete: ordering machinery inside the crate (Phase 3, step 4)

See section 4 for the full list across crates.

## 4. Ordering Machinery

This is the "complexity around ordering messages between originators" that the charter names.
It spans four crates. Total order per topic replaces originator ordering.

| Path | What | Lines | Action |
| --- | --- | --- | --- |
| `crates/xmtp_api_d14n/src/protocol/order.rs` | Envelope ordering by originator (XIP-49) | 565 | Delete |
| `crates/xmtp_api_d14n/src/queries/stream/ordered.rs` | Stream combinator over `order.rs` | 168 | Delete |
| `crates/xmtp_api_d14n/src/protocol/sort/` | Causal (vector-clock) and timestamp sort | 309 | Delete |
| `crates/xmtp_api_d14n/src/protocol/resolve/` | `network_backoff.rs`: backoff when a dependency is missing | 200 | Delete |
| `crates/xmtp_api_d14n/src/protocol/traits/cursor_store.rs` | `CursorStore` trait | 469 | Delete |
| `crates/xmtp_api_d14n/src/protocol/in_memory_cursor_store.rs` | In-memory `CursorStore` | 259 | Delete |
| `crates/xmtp_api_d14n/src/protocol/traits/dependency_resolution.rs` | `depends_on` resolution | 133 | Delete |
| `crates/xmtp_api_d14n/src/protocol/extractors/depends_on.rs` | `depends_on` extractor | 43 | Delete |
| `crates/xmtp_mls/src/cursor_store.rs` | DB-backed `CursorStore`; `builder.rs` and the 3 bindings pass it into client construction | 210 | Delete |
| `crates/xmtp_proto/src/types/global_cursor.rs` | `GlobalCursor` (originator to sequence map) | 353 | Delete after step 5 |
| `crates/xmtp_proto/src/types/topic_cursor.rs` | Per-topic cursor | 143 | Simplify to a scalar sequence |
| `crates/xmtp_proto/src/types/cursor.rs` | `Cursor` type (imports xmtpv4 today) | 170 | Simplify |
| `crates/xmtp_proto/src/types/cursor_list.rs` | Cursor list | 47 | Delete |
| `crates/xmtp_proto/src/types/orphaned_envelope.rs` | Envelope waiting on a dependency | 71 | Delete |
| `crates/xmtp_proto/src/traits/vector_clock.rs` | Vector clock trait | 23 | Delete |
| `crates/xmtp_db/src/encrypted_store/migration_test/originator_id_refresh_state.rs` | Migration test for originator IDs on `refresh_state` | 393 | Delete |
| `crates/xmtp_db/src/encrypted_store/refresh_state.rs` | Holds originator-keyed cursors | 938 (partial) | Simplify to one cursor per topic |
| `crates/xmtp_db/src/encrypted_store/icebox.rs` + `icebox/types.rs` | Envelopes whose dependencies have not arrived | 869 + 64 | Delete. Per-topic total order removes the need. |
| `crates/xmtp_mls/src/groups/tests/test_message_dependencies.rs` | d14n-only `depends_on` tests (gated by `if_d14n!` in `crates/xmtp_mls/src/groups/tests/mod.rs:27`) | 164 | Deleted in the Phase 0 test PR |
| `crates/xmtp_mls/src/groups/subscriptions.rs` | v4 paths: `V3OrD14n` decode, icebox handling, xmtpv4 test imports at line 191 | 519 (partial) | Simplify in Phase 3 |
| `depends_on` impls in `crates/xmtp_proto/src/impls.rs`, `crates/xmtp_proto/src/types/topic.rs`, `crates/xmtp_proto/src/types/group_message.rs` | `depends_on` plumbing on kept types | part | Delete with step 4 |

**Order matters.** Step 4 must follow step 3: the v3 client threads `CursorStore` through
every call. `GlobalCursor` is used by kept types and must be replaced by `Cursor` in
`crates/xmtp_proto/src/types/group_message.rs`, `crates/xmtp_proto/src/types/welcome_message.rs`,
`crates/xmtp_proto/src/types/message_metadata.rs`, `crates/xmtp_mls/src/subscriptions/process_message/factory.rs`,
`crates/xmtp_db/src/encrypted_store/group_message.rs`, `crates/xmtp_mls/src/groups/welcome_sync.rs`,
`crates/xmtp_mls/src/subscriptions/stream_messages/types.rs`, and
`crates/xmtp_mls/src/subscriptions/stream_router.rs` (step 5) before `global_cursor.rs` goes.
`apps/db_tools/src/tasks/db_bench.rs:21,297` also uses `GlobalCursor` and changes with it.

## 5. The `d14n` Cargo Feature

Removing the feature means picking one arm at every site. There are 61 `feature = "d14n"`
or `feature = "v3"` occurrences in Rust source.

Cargo manifests (delete the feature lines):

- `crates/xmtp_api_d14n/Cargo.toml:81-82` (`v3 = []`, `d14n = []`)
- `crates/xmtp_api/Cargo.toml:56-57`
- `crates/xmtp_mls/Cargo.toml:54-55`
- `bindings/mobile/Cargo.toml:91`

Macros (delete both): `crates/xmtp_common/src/macros.rs:93-110` defines `if_d14n!` and `if_v3!`.

`cfg` sites in library code:

- `crates/xmtp_api_d14n/src/definitions.rs:11-17` (the `ApiClient` alias switch)
- `crates/xmtp_api_d14n/src/test/definitions.rs:76,92`
- `crates/xmtp_api/src/test_utils.rs:5-13` (test client aliases)
- `crates/xmtp_mls/src/subscriptions/mod.rs:762,817` (two stream-setup paths)
- `crates/xmtp_mls/src/subscriptions/mod.rs:22-53` (test module gates)
- `crates/xmtp_mls/src/lib.rs:22` (`migration_tests`, deleted in Phase 0)
- `crates/xmtp_mls/src/groups/tests/mod.rs:27` (`test_message_dependencies`, deleted in Phase 0)
- `crates/xmtp_mls/src/subscriptions/stream_router.rs:1041`, `router_callbacks.rs:175,182`, `catch_up.rs:663`
- `crates/xmtp_mls/src/registration_visible/tests.rs:59`
- `bindings/mobile/src/mls/test_utils.rs:127`, `bindings/mobile/src/mls/tests/streaming.rs:698`, `bindings/mobile/src/mls/tests/lifecycle.rs:41`, `bindings/mobile/src/mls/device_sync/tests.rs:257`

`cfg_attr(..., ignore)` sites in tests (41). After the cutover the attribute is removed and
the test runs or is deleted:

| File | Sites |
| --- | --- |
| `crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs` | 13 |
| `crates/xmtp_mls/src/groups/tests/test_commit_log_fork_detection.rs` | 7 |
| `crates/xmtp_mls/src/groups/tests/test_commit_log_readd_requests.rs` | 5 |
| `crates/xmtp_mls/src/groups/tests/test_commit_log_local.rs` | 4 |
| `crates/xmtp_mls/src/groups/tests/test_welcome_pointers.rs` | 3 |
| `bindings/mobile/src/mls/device_sync/tests.rs` | 2 |
| `bindings/mobile/src/mls/tests/{streaming,static_methods,group_management,dms}.rs` | 4 |
| `crates/xmtp_mls/src/client.rs`, `subscriptions/stream_conversations.rs`, `subscriptions/stream_all/tests.rs` | 3 |

Test recipes, CI matrix, nextest profiles:

| Path | What | Action |
| --- | --- | --- |
| `justfile:73-93` | `nix-test`, `_test-all`, `_test-v3`, `_test-d14n` | Collapse to one `just test` |
| `.config/nextest.toml:30-45` | `[profile.ci-d14n]`, junit, overrides | Delete the profile |
| `nix/ci-checks.nix:27,29` | `d14n` and `wasm-d14n` nextest attrs | Delete both |
| `nix/package/nextest.nix:6,49,53,54` | `d14n ? false` and branches | Remove the argument |
| `nix/package/wasm-nextest.nix:12,54,55,86,93,94` | Same for wasm; `-p xmtp_api_d14n` in `wasmPackages` | Remove the argument; rename the package |
| `bindings/wasm/wasm.just:31-62` | `test-d14n`, `nix-test-d14n`, package list | Collapse |
| `.github/workflows/test-workspace.yml:25-33` | Builds v3 and d14n, merges lcov | One build, one tracefile |
| `.github/workflows/ignored-tests-tracker.yml:31-39` | `native-d14n` and `wasm-d14n` rows | Delete rows |
| `.github/workflows/release-notes.yml:135,232` | `xmtp_api_d14n` in the crate list | Rename |
| `.github/workflows/cross-test.yml:42`, `.github/workflows/push-xdbg.yml:16` | Path trigger on `crates/xmtp_api_d14n/**` | Disabled in Phase 1 |

## 6. Streaming

The XIP-83 bidi path becomes the only stream path. The legacy per-stream subscriptions go.

### 6.1 Keep

| Path | What | Lines | Action |
| --- | --- | --- | --- |
| `crates/xmtp_api_d14n/src/queries/bidi.rs`, `bidi_transport.rs`, `bidi_transport_props.rs` | XIP-83 client | 8,061 | Keep; one binding |
| `crates/xmtp_mls/src/subscriptions/stream_router.rs` | Router; `V3Binding`, `V3ProtoGroupMessage`, `V3ProtoWelcomeMessage` | 1,685 | Keep; rename the binding types to the one backend |
| `crates/xmtp_mls/src/subscriptions/router_callbacks.rs` | Callback streams; the `XMTP_BIDI_STREAMS_ENABLED` gate and latch at lines 101-104 and after | 865 | Keep; delete the gate and the latch; bidi is the default |
| `crates/xmtp_mls/src/subscriptions/catch_up.rs` | Bidi catch-up | 861 | Keep; spec 004 decides the final shape |
| `crates/xmtp_mls/src/subscriptions/process_message.rs`, `process_welcome.rs`, `process_message/factory.rs` | Message and welcome processing | 1,287 | Keep; simplify cursor types |
| `crates/xmtp_mls/src/subscriptions/bidi_tests.rs`, `bidi_fuzz_tests.rs`, `router_callbacks_tests.rs`, `stream_router_tests.rs` | Bidi tests | 2,886 | Keep. Port the backend-agnostic assertions from `d14n_bidi_tests.rs` into `bidi_tests.rs` before Phase 0 deletes it (see the test-deletion plan). |

### 6.2 Delete: the legacy streaming stack (Phase 3, step 8)

| Path | What | Lines |
| --- | --- | --- |
| `crates/xmtp_mls/src/subscriptions/stream_messages.rs` + `stream_messages/` | Legacy per-group message stream | 659 + 364 |
| `crates/xmtp_mls/src/subscriptions/stream_conversations.rs` | Legacy welcome stream | 686 |
| `crates/xmtp_mls/src/subscriptions/stream_all.rs` + `stream_all/tests.rs` | Legacy all-messages stream | 215 + 1,098 |
| `crates/xmtp_mls/src/subscriptions/watchdog.rs` | Liveness watchdog for legacy subscriptions | 762 |
| `crates/xmtp_mls/src/subscriptions/d14n_compat.rs` | v3-or-d14n decode shim | 141 |
| `crates/xmtp_mls/src/subscriptions/d14n_bidi_tests.rs` | d14n-only bidi tests | 404 (Phase 0) |
| `crates/xmtp_mls/src/migration_tests.rs` | v3 to d14n migration tests | 146 (Phase 0) |
| `crates/xmtp_mls/src/registration_visible/` | Uses `xmtp_api_d14n::d14n::QueryEnvelopes` at `mod.rs:3,65`; the network-severed test is d14n-gated | 369 |
| `crates/xmtp_mls/src/subscriptions/mod.rs:762,817` | Two stream-setup paths under `cfg(d14n)` | about 110 |

Subtotal deleted in Phase 3: about 3,900 lines plus `registration_visible` (369).

## 7. Protos And `xmtp_proto`

`crates/xmtp_proto` totals 73,789 lines; `src/gen` is 68,008 across 49 files.

### 7.1 The move (Phase 1)

1. Create `proto/`. Copy every `.proto` still referenced by kept code into it. This includes
   the xmtpv4 envelope types that kept code uses today: `crates/xmtp_proto/src/impls.rs`,
   `crates/xmtp_proto/src/api_client/impls.rs`, `crates/xmtp_proto/src/types/topic.rs`,
   `crates/xmtp_proto/src/types/cursor.rs`, `crates/xmtp_proto/src/types/global_cursor.rs`,
   `crates/xmtp_mls/src/groups/subscriptions.rs`, `crates/xmtp_mls/src/registration_visible/mod.rs`,
   and `crates/xmtp_mls/src/subscriptions/d14n_compat.rs`.
2. Add the new `xmtp.backend.v1` file from `docs/self-hosted/backend.proto`.
3. Rewrite `crates/xmtp_proto/build.rs:51-80` (`clone_proto_repos`) to read `proto/`. Drop the
   `xmtp/proto` clone at line 72. Keep the grpc-gateway and googleapis clones (lines 53-63)
   only until Phase 3 removes the annotations.
4. Delete `crates/xmtp_proto/proto_version`. Rewrite `dev/gen_protos.sh` (it resolves a
   branch SHA from `xmtp/proto` today).
5. Regenerate. Point every consumer at the new generated module. Delete the old generated tree.
6. Delete `crates/xmtp_proto/src/gen/xmtp.keystore_api.v1.rs` and `.serde.rs` (6,603 lines,
   zero consumers) and their entries at `crates/xmtp_proto/src/gen/mod.rs:40-43`.
7. Delete `.github/workflows/nightly-protos.yml`.

### 7.2 Per-package status

| Package | Files | Lines | Consumers outside `src/gen` | Phase and action |
| --- | --- | --- | --- | --- |
| `xmtp.keystore_api.v1` | 2 | 6,603 | none | 1: delete |
| `xmtp.xmtpv4.*` (envelopes, message_api, metadata_api, payer_api, gateway_api) | 9 | 11,638 | `xmtp_api_d14n`, plus the kept files in 7.1 | 1: copy into `proto/`; 3 step 6: delete after step 5 |
| `xmtp.migration.api.v1` | 2 | 301 | `crates/xmtp_api_d14n/src/endpoints/d14n/fetch_d14n_cutover.rs`, `apps/xnet` | 3 step 6: delete |
| `xmtp.message_api.v1` | 2 | 2,387 | `crates/xmtp_proto/src/api_client.rs:1-4` re-exports 8 v2 types; `SortDirection` at `crates/xmtp_api/src/mls.rs:760` and `crates/xmtp_mls/src/groups/commit_log.rs:39`; `PublishRequest` at `crates/xmtp_api_grpc/src/grpc_client/client.rs:341` | 3 step 3: delete after moving `SortDirection` into `xmtp.backend.v1` and removing the re-exports |
| `xmtp.mls.api.v1` | 2 | 9,208 | Heavy | 3 step 3: delete with the v3 arm. The `xmtp.backend.v1` payload messages replace the input messages. |
| `xmtp.mls_validation.v1` | 2 | 2,330 | `apps/mls_validation_service`, `apps/xnet` | 2: delete |
| `grpc.gateway.protoc_gen_openapiv2.options` | 1 | 1,255 | Annotation types; only `xmtp.mls.api.v1.rs` references them in generated Rust | 3: delete |
| `google.api` | 1 | 404 | Annotation types | 3: delete |
| `xmtp.identity.api.v1`, `xmtp.identity.associations`, `xmtp.identity` | 6 | 5,436 | Heavy | Keep. `identity.api.v1` carries HTTP annotations (`docs/self-hosted/existing/proto.md`, section 2.1); drop them with the gateway files. |
| `xmtp.mls.message_contents` | 4 | 9,888 | Heavy | Keep |
| `xmtp.message_contents` | 2 | 8,641 | Heavy | Keep |
| `xmtp.mls.database` | 2 | 5,333 | Heavy | Keep |
| `xmtp.device_sync.*` | 12 | 4,476 | Heavy | Keep |

Also edit `crates/xmtp_proto/src/gen/mod.rs` (108 lines) and the `server_mod_attribute`
entries in `crates/xmtp_proto/build.rs:13-47` for each deleted package.

### 7.3 Hand-written `xmtp_proto` modules

| Path | Lines | Action |
| --- | --- | --- |
| `crates/xmtp_proto/src/api_client.rs` | 262 | Replace; re-exports `message_api::v1` at line 1 |
| `crates/xmtp_proto/src/api_client/impls.rs` | 451 | Replace; uses xmtpv4 types |
| `crates/xmtp_proto/src/api_client/stats.rs` | 119 | Keep |
| `crates/xmtp_proto/src/traits.rs` | 386 | Keep: `Endpoint`, `Query`, `ApiBuilder`, `ApiClientError` |
| `crates/xmtp_proto/src/traits/combinators/v3_paged.rs` | 304 | Delete |
| `crates/xmtp_proto/src/traits/combinators/retry.rs`, `ignore.rs` | 305 | Keep |
| `crates/xmtp_proto/src/impls.rs` | 116 | Simplify; xmtpv4 and `depends_on` impls go |
| `crates/xmtp_proto/src/types/global_cursor.rs`, `cursor_list.rs`, `orphaned_envelope.rs`, `traits/vector_clock.rs` | 494 | Delete (section 4) |
| `crates/xmtp_proto/src/types/cursor.rs`, `topic_cursor.rs` | 313 | Simplify |
| `crates/xmtp_proto/src/types/topic.rs`, `group_id.rs`, `installation_id.rs`, `welcome_message.rs`, `group_message.rs`, `message_metadata.rs`, `app_version.rs` | about 1,700 | Keep; strip xmtpv4 imports and `depends_on` |
| `crates/xmtp_proto/src/impls/update_dedupe.rs` | 136 | **verify**: dedupes `GroupUpdated` fields; may be unneeded with one backend |

## 8. Payer And Blockchain

### 8.1 Payer

| Path | What | Action |
| --- | --- | --- |
| `crates/xmtp_proto/src/gen/xmtp.xmtpv4.payer_api.rs` + `.serde.rs` | Payer gRPC types (656 lines) | Delete with xmtpv4 |
| `crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs`, `get_nodes.rs` | `PayerApi` calls | Delete with the d14n arm |
| `crates/xmtp_configuration/src/common/d14n.rs:19` | `PAYER_WRITE_FILTER` | Delete |
| `crates/xmtp_api/src/mls.rs:374,653` | Uses `PAYER_WRITE_FILTER` to route writes | Remove the filter |
| `crates/xmtp_api_d14n/src/middleware/multi_node_client/client.rs:122,159,200`, `queries/client_bundle.rs:8,239` | Same | Delete with the modules |
| `apps/xmtp_debug/src/args.rs:6,373-421,490` | `--xmtpd-gateway-url`, perf gateway, payer filter | Retarget xdbg (section 10) |
| `crates/xmtp_configuration/src/common/api.rs` | `GATEWAY` and `PERF_GATEWAY` consts in the URL structs | Delete |

### 8.2 Blockchain

**Keep**: `crates/xmtp_id/src/scw_verifier/` (636 lines: `mod.rs`, `chain_rpc_verifier.rs`,
`remote_signature_verifier.rs`, `chain_urls_default.json`, `signature_validation.hex`).
Smart contract wallet signature verification is an SDK feature. It needs an EVM RPC client,
so `alloy` stays (`Cargo.toml:36`, `crates/xmtp_id/Cargo.toml:21` with `sol-types`).

**Anvil.** The SCW tests use the `docker_smart_wallet` fixture in `crates/xmtp_id/src/utils/test.rs:44`
and `scw_verifier/mod.rs:226-232` reads `ANVIL_URL` or `DockerUrls::ANVIL`
(`crates/xmtp_configuration/src/common/api.rs:30`). Phase 3 changes the tests to spawn a
local `anvil` from Rust with the alloy node-bindings feature (dev-only). Findings:

- The `anvil` binary is in the Nix dev shells: `flake.nix:20` pins `foundry.nix`, and
  `foundry-bin` is in `nix/shells/rust.nix:44` and `nix/shells/local.nix:87`.
- `crates/xmtp_id/Cargo.toml:79-83` already enables `alloy/provider-anvil-api` and
  `alloy/provider-anvil-node` under the `test-utils` feature. The dev-dependency at
  `crates/xmtp_id/Cargo.toml:49` onward enables `providers`, `rpc`, `network`, `provider-anvil-api`, and `provider-anvil-node`.

So the `anvil` docker service goes in Phase 3 with no charter exception. The alloy
provider and RPC subtree stays in `Cargo.lock` as a dev dependency.

**Delete** (blockchain used only by xmtpd, payer, and the node registry):

| Path | What | Lines |
| --- | --- | --- |
| `apps/xnet/lib/src/contracts.rs` | Node-registry contract bindings | 261 |
| `apps/xnet/lib/src/node_provisioner.rs` | Registers nodes on chain, funds payers | about 180 |
| `apps/xnet/lib/Cargo.toml:15-23` | alloy features `signer-local`, `provider-http`, `network`, `rpc-types-eth`, `provider-anvil-api`, `contract` | drop with xnet |
| `dev/docker/anvil.Dockerfile`, the `anvil` service in `dev/docker/docker-compose.yml:57`, the `chain` service in `dev/docker/docker-compose-d14n.yml:41` | Local chains | Phase 3 |
| `crates/xmtp_configuration/src/common/api.rs:30,152` | `DockerUrls::ANVIL`, `GrpcUrlsToxic::ANVIL` | Phase 3 |

`apps/mls_validation_service/Cargo.toml:22` (`alloy.workspace = true`) moves to the shared
validation crate.

## 9. v3 Transport

`crates/xmtp_api_grpc` (2,075 lines) is transport only. It holds no v3 endpoint definitions.

| Path | Lines | Verdict |
| --- | --- | --- |
| `crates/xmtp_api_grpc/src/grpc_client/client.rs` | 379 | Keep. One v3 leak at line 341 (`use xmtp_proto::xmtp::message_api::v1::PublishRequest`, in a test). Remove it in Phase 3 step 3. |
| `crates/xmtp_api_grpc/src/grpc_client/native.rs` | 248 | Keep |
| `crates/xmtp_api_grpc/src/grpc_client/wasm.rs` | 40 | Keep; wasm CI is off from Phase 1 |
| `crates/xmtp_api_grpc/src/streams/` | 1,064 | Keep |
| `crates/xmtp_api_grpc/src/error.rs` | 124 | Keep |
| `crates/xmtp_api_grpc/src/grpc_client/test/clients.rs` | 154 | Replace |

## 10. Apps

| Path | What | Backend | Lines | Verdict |
| --- | --- | --- | --- | --- |
| `apps/xnet` | Docker orchestrator for xmtpd test networks | d14n only | 7,995 | **Delete in Phase 0** (test-deletion PR). See 10.1. |
| `apps/xmtp_debug` (xdbg) | Network debug and load-generation CLI | v3 and d14n | 11,571 | **Keep, retarget in Phase 3.** See 10.2. |
| `apps/mls_validation_service` | Standalone gRPC validation service | Backend-side | 1,058 | **Split in Phase 1 and 2.** See 10.3. |
| `apps/keepalive-probe` | gRPC keepalive probe; opens `MlsApi/SubscribeGroupMessages` streams (`apps/keepalive-probe/src/main.rs:273-285`) | v3 | 647 | Keep, retarget the subscribe RPC in Phase 3. Keep `.github/workflows/test-keepalive-probe.yml`, `push-keepalive-probe.yml`, `nix/package/keepalive-probe-check.nix`. |
| `apps/db_tools` | SQLite migration and bench tools | none | 1,438 | Keep. `apps/db_tools/src/tasks/db_bench.rs:21,297` changes with `GlobalCursor`. |
| `apps/error_glossary` | Error glossary generator (`clap`, `syn`, `walkdir`) | none | 511 | Keep |
| `apps/android` | `xmtpv3_example` Gradle app; 0 Rust lines; in `Cargo.toml:12` `exclude` | none | 0 | **verify** whether it is still used; not in scope |

### 10.1 `apps/xnet` (Phase 0)

Owner decision: delete all of `apps/xnet` in the Phase 0 test-deletion PR, before
`xmtp_api_d14n` changes. The same commit must change:

| Path | Line | What |
| --- | --- | --- |
| `Cargo.toml` | 5 | `"apps/xnet/*"` in `members` |
| `Cargo.toml` | 12 | the two `apps/xnet` entries in `exclude` (the first names a `Cargo.toml` that does not exist on this checkout; the second is `apps/xnet/assets`) |
| `Cargo.toml` | 233-234 | `xnet-cli`, `xnet-lib` workspace dependencies |
| `flake.nix` | 61 | `inherit (self'.packages) xnet-cli;` |
| `nix/apps.nix` | 7 | `xnet-cli = pkgs.callPackage ./package/xnet-cli.nix { };` |
| `nix/package/xnet-cli.nix` | all | Delete |
| `nix/lib/filesets.nix` | 24-25, 54, 61, 68 | `apps/xnet/cli`, `apps/xnet/lib`, `apps/xnet/.gitkeep`, `apps/xnet/lib/signers.txt` |
| `nix/package/mls_validation_service.nix` | 36 | `(root + /apps/xnet/.gitkeep)` |
| `dev/docker/xnet.toml` | all | Delete |

Its 6 test files go with it. Cargo dependencies only xnet uses: section 15.

### 10.2 `apps/xmtp_debug` (keep, retarget)

Owner decision: "keep xmtp_debug as an app, and delete relevant functionality from it
(backend switching, etc) as we delete dependency code." Its CI (`test-xdbg.yml`,
`push-xdbg.yml`, `cross-test.yml`) is disabled in Phase 1, not deleted. In Phase 3:

- Delete backend selection and gateway or payer URL handling in `apps/xmtp_debug/src/args.rs`
  (`--xmtpd-gateway-url`, perf gateway, `PAYER_WRITE_FILTER`).
- Delete d14n paths in `apps/xmtp_debug/src/app/clients.rs`, `app/test.rs`,
  `app/generate/identity.rs` (`MultiNodeClient`, `SubscribeTopics`, `QueryEnvelopes`).
- Delete dev, staging, and production URL constants in `apps/xmtp_debug/src/constants.rs`.
- Re-verify its dependency list (`redb`, `speedy`, and the rest of `apps/xmtp_debug/Cargo.toml`)
  with `cargo tree -i` after the retarget. Nothing is dropped now.

### 10.3 `apps/mls_validation_service` (Phase 1 extract, Phase 2 delete)

| Path | Lines | Verdict |
| --- | --- | --- |
| `apps/mls_validation_service/src/handlers.rs` | 595 | **Moves** to the shared crate (Phase 1). Key package validation, association state, inbox-id validation. Strip the `ValidationApi` tonic impl; keep the functions and the 8 logic tests. |
| `apps/mls_validation_service/src/cached_signature_verifier.rs` | 293 | **Moves.** LRU cache over `SmartContractSignatureVerifier`. |
| `apps/mls_validation_service/src/main.rs` | 91 | Dies (Phase 2). tonic server, clap, tracing, signals. |
| `apps/mls_validation_service/src/health_check.rs` | 19 | Dies. warp health server. |
| `apps/mls_validation_service/src/config.rs` | 38 | Dies. Replaced by the Phase 2 TOML config. |
| `apps/mls_validation_service/src/version.rs`, `apps/mls_validation_service/build.rs` | 3 + build | Dies. vergen stamp. |
| `apps/mls_validation_service/Cargo.toml` | `[[bin]]` (14), `warp` (40), `grpc_server_impls` (46) | Dies. |

Also delete in Phase 2: `dev/validation_service/Dockerfile`, `dev/validation_service/local.Dockerfile`,
`dev/build_validation_service`, `dev/build_validation_service_local`,
`nix/package/mls_validation_service.nix`, `.github/workflows/release-mls-validation-service.yml`,
the `validation` docker service, the `nix build .#validation-service-image` step at
`justfile:130`, the `validation-service-image` derivations in `nix/musl-docker.nix:19-51`
(retarget to the backend image), and `Cargo.toml:16` in `default-members` (swap for
`apps/backend`). `xmtp_proto`'s `grpc_server_impls` feature (`crates/xmtp_proto/Cargo.toml:88`)
has one user today; the backend takes it over.

`crates/xmtp_api/src/scw_verifier.rs` (52 lines) is the remote-verifier bridge to the
service. **verify** whether it changes or dies when verification moves in-process.

## 11. Client Config, Bindings, SDKs

Change the option objects to take a backend URL. Do not remove the app-facing option
objects; the charter requires app code to keep working.

| Path | What | Lines | Action |
| --- | --- | --- | --- |
| `crates/xmtp_configuration/src/common/env.rs` | `XmtpEnv` enum: 3 centralized and 4 decentralized variants, `default_api_url()`, `is_d14n()` | 87 | Collapse to a URL, or a 1-variant enum for SDK compatibility |
| `crates/xmtp_configuration/src/common/api.rs` | `GrpcUrls*`, `DockerUrls`, `MULTI_NODE_TIMEOUT_MS`, wasm and native URL split for envoy | 153 | Cut to about 30 lines: `GRPC_PAYLOAD_LIMIT`, `LOCALHOST`, a local test URL, `DeviceSyncUrls` |
| `crates/xmtp_common/src/macros.rs:111-126` | `if_dev!`, `if_local!`; used only at `crates/xmtp_configuration/src/common/api.rs:42,50` | 18 | Delete with the URL structs |
| `crates/xmtp_mls/Cargo.toml:56`, `crates/xmtp_api_d14n/Cargo.toml:83`, `crates/xmtp_configuration/Cargo.toml:18`, `crates/xmtp_proto/Cargo.toml:81`, `bindings/mobile/Cargo.toml:92` | `dev` feature chain | 5 | Delete |
| `crates/xmtp_mls/src/utils/test/definitions.rs:21-49` | 7 test-client aliases; `FeatureSwitchedTestClientCreator` aliased as `DefaultTestClientCreator` at line 49 | 49 | Collapse to one `TestClient` and one creator |
| `crates/xmtp_mls/src/utils/test/tester_utils.rs:206-214` | `ApiEndpoint::{Local,Dev,Toxic}` branching; cursor-store plumbing | 866 (partial) | One backend plus optional toxiproxy |
| `crates/xmtp_mls/src/utils/test/mod.rs:45,66` | `DevOnlyTestClientCreator`, `LocalOnlyMigrationClientCreator` | 2 sites | Delete |
| `crates/xmtp_mls/src/utils/bench/clients.rs:3,26-29` | Bench clients by env | 2 sites | Simplify |
| `crates/xmtp_mls/src/definitions.rs:11` | `XmtpApiClient` alias | 1 | Point at the renamed crate |
| `bindings/node/src/client/options.rs:140-150` | napi `XmtpEnv` (7 variants) | about 25 | Reduce variants; keep the field name |
| `bindings/node/src/client/backend.rs:16,78-79,87-93,104-122` | `gateway_host`, `maybe_v3_host`, `build_optional_d14n` | about 40 | Single URL; drop the `gatewayHost` getter |
| `bindings/node/src/client/create_client.rs:287-320` | `v3_host`, `gateway_host`, `build_optional_d14n` | about 10 | Single URL |
| `bindings/wasm/src/client.rs:187-207` | wasm `XmtpEnv` and `From` | about 25 | Same as node |
| `bindings/wasm/src/client/backend.rs` | `ClientBundleBuilder`, `AuthCallback`, `MessageBackendBuilder` | about 60 | Repoint at the renamed crate |
| `bindings/mobile/src/mls.rs:139-170` | `create_client(v3_host, gateway_host, ...)`; the doc comment at line 139 says `gateway_host` enables d14n | about 45 | Single host. This is a uniffi surface change; regenerate `sdks/ios/Sources/XMTPiOS/Libxmtp/xmtpv3.swift` and the Kotlin bindings. |
| `bindings/mobile/src/mls/gateway_auth.rs`, `bindings/node/src/client/gateway_auth.rs`, `bindings/wasm/src/client/gateway_auth.rs` | Auth FFI | about 280 | Keep the shape |
| `sdks/ios/Sources/XMTPiOS/XMTPEnvironment.swift` | `enum XMTPEnvironment`, `customLocalAddress`, `customHistorySyncUrl` | about 60 | Keep the type; values become URLs |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPEnvironment.kt` | Same, Kotlin | about 25 | Same |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:508`, `Dm.kt:506`, `Conversations.kt:158` | `// TODO: Handle multiple ... with d14n` | 3 | Delete the comments |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableGroup.kt:35`, `sdks/ios/Sources/XMTPiOS/UnstableGroup.swift:24` | "Post-d14n" comments | 2 | Reword |
| `sdks/android/dev/local/docker-compose.yml` | Android-local stack: `node`, `validation`, `anvil`, `history-server`, `db`, `mlsdb` | about 50 | Rewrite for the `backend` service |

`crates/xmtp_mls_common` (11,397 lines) has no backend-specific code. Keep all of it.

`legacy_delegated_signature` handling in `crates/xmtp_id/src/associations/` is XIP-46
identity history, not backend code. Keep it.

## 12. Docker, Dev Scripts, Nix, CI

### 12.1 Docker (Phase 3, step 11)

`dev/docker/docker-compose.yml` (105 lines) and `dev/docker/docker-compose-d14n.yml` (169 lines).

| Service | File | Action |
| --- | --- | --- |
| `node` (line 5), `node-web` (27), `validation` (46), `anvil` (57), `mlsdb` (78) | `docker-compose.yml` | Delete; `node-web` takes `dev/docker/envoy.yaml` and `anvil` takes `dev/docker/anvil.Dockerfile` |
| `db` (72), `history-server` (65), `toxiproxy` (84) | `docker-compose.yml` | Keep; add `backend` |
| `redis`, `replicationdb`, `chain`, `register-node-native`, `enable-node-native`, `xmtpd`, `gateway` | `docker-compose-d14n.yml` | Delete the file |

`dev/docker/toxiproxy/config.json` lists 6 proxies (`node-go`, `grpc-web`, `xmtpd`, `gateway`,
`history-server`, `anvil`); only `history-server` and a new `backend` proxy survive. Delete
`dev/docker/compose-v3` and `dev/docker/up-v3`. Simplify `dev/docker/compose`, `dev/docker/up`
(lines 18-29 handle the validation image), and `dev/docker/local.env` (39 lines of xmtpd and
anvil keys).

### 12.2 dev scripts

| Path | Phase | Action |
| --- | --- | --- |
| `dev/gen_protos.sh` | 1 | Rewrite for `proto/` |
| `dev/drivers/` (`cross_talk_test`, `cross_version_test`, `xdbg_driver_lib`; 1,384 lines) | 1 | Delete. Reason: the drivers fetch `release/*` branches and nightly tags, build those xdbg versions, and run them against one shared backend. No pre-transition release can reach the new backend, so the harness cannot be reused after Phase 3, and its CI is off from Phase 1. A new harness is written when two post-transition releases exist. |
| `dev/build_validation_service`, `dev/build_validation_service_local`, `dev/validation_service/` | 2 | Delete |
| `dev/docker/compose-v3`, `dev/docker/up-v3`, `dev/docker/anvil.Dockerfile`, `dev/docker/envoy.yaml` | 3 | Delete |
| `dev/docker/xnet.toml` | 0 | Delete with xnet |
| `dev/docker/compose`, `dev/docker/up`, `dev/docker/down`, `dev/docker/local.env` | 3 | Simplify |
| `dev/up:49`, `dev/nix-up`, `dev/direnv-up` | 2 | Stop calling the validation-service build |
| `dev/test/browser-sdk` | 1 | Disable with the browser SDK |
| `dev/bench`, `dev/llvm-cov`, `dev/flamegraph`, `dev/nix-shell`, `dev/docs`, `dev/gen-error-glossary` | - | Keep |

### 12.3 Nix

| Path | Phase | Action |
| --- | --- | --- |
| `nix/package/xnet-cli.nix`, `nix/apps.nix:7`, `flake.nix:61`, `nix/lib/filesets.nix:24-25,54,61,68`, `nix/package/mls_validation_service.nix:36` | 0 | xnet references (10.1) |
| `nix/package/cross-talk-test`, `nix/package/cross-version-test`, `nix/package/xdbg-driver-lib`, `flake.nix:76-78` | 1 | Delete with `dev/drivers` |
| `nix/package/mls_validation_service.nix`, `nix/musl-docker.nix:19-51` | 2 | Replace with a `backend` image derivation |
| `nix/package/xdbg.nix`, `nix/package/xdbg-check.nix`, `nix/apps.nix:6`, `nix/ci-checks.nix:39` | 3 | Keep; xdbg stays. Update after the retarget. |
| `nix/ci-checks.nix:27,29`, `nix/package/nextest.nix`, `nix/package/wasm-nextest.nix` | 3 | Remove the `d14n` argument and attrs |
| `flake.nix` | each | Drop the deleted package and app attrs |

### 12.4 CI workflows

Disable in Phase 1 (do not delete). `test-xdbg.yml` is called at `.github/workflows/test.yml:157,179`
and `cross-test.yml` is read by `.github/workflows/release-gate-plan.yml:58` (the cross-test
gate). Deleting either needs edits to those callers first. `push-xdbg.yml` has no caller.

| File | Note |
| --- | --- |
| `.github/workflows/test-xdbg.yml`, `.github/workflows/push-xdbg.yml`, `.github/workflows/cross-test.yml` | xdbg; edit `test.yml` and `release-gate-plan.yml` when they go |
| `.github/workflows/test-wasm.yml`, `.github/workflows/lint-wasm.yml`, `.github/workflows/release-wasm.yml` | wasm |
| `.github/workflows/test-browser-sdk.yml`, `.github/workflows/release-browser-sdk.yml` | browser SDK |
| `.github/workflows/lint-js.yml` | **verify**: it runs `just js bindings` and lints all JS SDKs; keep if the node SDK needs it |

Delete: `.github/workflows/nightly-protos.yml` (Phase 1),
`.github/workflows/release-mls-validation-service.yml` (Phase 2).

Edit in Phase 3: `.github/workflows/test-workspace.yml:25-33`,
`.github/workflows/ignored-tests-tracker.yml:31-39`, `.github/workflows/release-notes.yml:135,232`,
and the `dev/docker/**` path filters in `.github/workflows/test.yml`. **verify**
`.github/workflows/release.yml:187,412` (cross-test gate comments) with the release-gate change.

### 12.5 Root config

| Path | Edit |
| --- | --- |
| `Cargo.toml:219` | Rename `xmtp_api_d14n` (Phase 3 step 9) |
| `Cargo.toml:5,12,233-234` | xnet (Phase 0) |
| `Cargo.toml:16` | `default-members`: swap `apps/mls_validation_service` for `apps/backend` (Phase 2) |
| `Cargo.toml:269-280` | `hpke-rs` fork patch. Owner: leave it. |
| `justfile:73-93,101-117,128-130` | Test recipes, `cross-test`, `cross-talk-test`, `_backend-up` |
| `.config/nextest.toml:30-45` | `ci-d14n` profile |
| `deny.toml:5` | `RUSTSEC-2026-0173` exception for `alloy-sol-macro`; stays with alloy |
| `TEST_SCENARIOS.md`, `ONBOARDING.md` | Update for the new service set; do not delete |

## 13. Ordered Deletion Plan

| Phase | What | Approx lines |
| --- | --- | --- |
| 0 | d14n-only test files (see the Phase 0 test-deletion plan) | 1,000 |
| 0 | `apps/xnet` with its Nix references, workspace entries, and xnet-only dependencies | 8,000 |
| 1 | `proto/` folder authoritative with the old generated tree deleted; `xmtp_proto` `keystore_api` generated files and `nightly-protos.yml` deleted with the proto move; shared crate scaffolded; `apps/backend` scaffold; CI workflows for xdbg, wasm, and the browser SDK disabled (not deleted; `test-xdbg.yml` is called by `test.yml`, and `cross-test.yml` is read by `release-gate-plan.yml`, so deleting them needs edits to those callers first) | 6,600 |
| 1 | `dev/drivers` (`cross_talk_test`, `cross_version_test`, `xdbg_driver_lib`) and the matching nix packages | 1,400 |
| 2 | Move the validation logic and its 8 logic tests into the shared crate, make the backend use it, then delete the `apps/mls_validation_service` shell (main, health, config); `xmtp.mls_validation.v1` protos; its nix package, release workflow, `dev/validation_service`, `dev/build_validation_service*` | 2,500 |
| 3 | Integration: collapse the `d14n` feature; delete the d14n arm (`endpoints/d14n`, `queries/d14n`, `queries/combined`, `middleware/multi_node_client`, `middleware/read_write_client`, `xmtp_configuration/src/common/d14n.rs`, `d14n_migration_cutover.rs`); replace the v3 arm (`endpoints/v3`, `queries/v3`, `xmtp_api/src/{mls,identity}.rs`, `message_api.v1` and `mls.api.v1` protos, `v3_paged.rs`); delete ordering machinery (`order.rs`, `sort/`, `resolve/`, cursor stores, `global_cursor.rs`, `vector_clock.rs`, `orphaned_envelope.rs`, `originator_id_refresh_state.rs`, `icebox.rs`); simplify `refresh_state.rs`; delete the xmtpv4 and migration copies from `proto/`; delete the legacy streaming stack (`stream_messages.rs`, `stream_conversations.rs`, `stream_all.rs`, `watchdog.rs`, `d14n_compat.rs`, the latch in `router_callbacks.rs`) and make XIP-83 bidi the default (remove the `XMTP_BIDI_STREAMS_ENABLED` opt-in); delete `crates/xmtp_mls/src/registration_visible/`; rename the crate; collapse env and URL config; simplify test harness aliases; `grpc.gateway` and `google.api` generated files; retarget xdbg; Docker | 43,000 |

### Phase 3 strict order

Each step removes the dependents of the next.

1. Collapse the `d14n` feature. Pick one arm at every `cfg` site (section 5). Remove the
   feature from 4 manifests, `if_d14n!` and `if_v3!` from `crates/xmtp_common/src/macros.rs`,
   the `ci-d14n` nextest profile, the `just test d14n` recipes, the nix `d14n` arguments, and
   the CI matrix rows.
2. Delete the d14n arm (code only, section 3.2).
3. Replace the v3 arm (section 3.3). Delete the `message_api.v1` and `mls.api.v1` generated
   files and `v3_paged.rs`.
4. Delete the ordering machinery including `icebox.rs` (section 4). This must follow step 3
   because the v3 client threads `CursorStore` through every call.
5. Simplify the kept consumers of `GlobalCursor` and the xmtpv4 types (section 4, section 7.1).
6. Delete the xmtpv4 and migration protos from `proto/`.
7. Collapse the bidi client to one binding and make it the default stream path.
8. Delete the legacy streaming stack (section 6.2) and `registration_visible`.
9. Rename the `xmtp_api_d14n` crate. Update `Cargo.toml:219`, the dependency lines in
   `xmtp_api`, `xmtp_mls`, and the 3 bindings, `nix/package/wasm-nextest.nix:55`,
   `bindings/wasm/wasm.just:31`, and `.github/workflows/release-notes.yml:135,232`.
10. Collapse env and URL config (section 11), then the binding option types, then regenerate
    the uniffi Swift and Kotlin surfaces.
11. Docker: delete `dev/docker/docker-compose-d14n.yml`; in `dev/docker/docker-compose.yml`
    remove `node`, `node-web`, `validation`, `mlsdb`, `anvil`; add `backend`; keep `db`,
    `toxiproxy`, `history-server`.

Also in Phase 3: `crates/xmtp_mls/src/groups/tests/test_message_dependencies.rs` (d14n-only
`depends_on` tests; the file itself is deleted in the Phase 0 test PR, its `if_d14n!` gate
at `crates/xmtp_mls/src/groups/tests/mod.rs:27` goes in step 1), the v4 paths in
`crates/xmtp_mls/src/groups/subscriptions.rs`, and the `depends_on` impls.

## 14. Keep List

Things that look deletable but must stay.

| Path | Why it must stay |
| --- | --- |
| `crates/xmtp_id/src/scw_verifier/` (636) | SCW signature verification is an SDK feature. Needs an EVM RPC client; `alloy` stays. |
| `crates/xmtp_id/src/associations/` (5,228) | XIP-46 identity. Backend-independent, including `legacy_delegated_signature`. |
| `crates/xmtp_id/src/key_package/` | Key package verification. The logic stays; the shared crate calls it. |
| `crates/xmtp_api_grpc` (2,075) | Generic gRPC transport. One v3 leak at `grpc_client/client.rs:341`. |
| `crates/xmtp_proto/src/traits.rs` (386) | `Endpoint`, `Query`, `ApiBuilder`, `ApiClientError`. The new client reuses the endpoint pattern. |
| `crates/xmtp_proto/src/types/` domain ID types (`topic`, `group_id`, `installation_id`, `welcome_message`, `group_message`, `message_metadata`, `app_version`) | Backend-independent after the cursor simplification. |
| Protos `xmtp.mls.message_contents`, `xmtp.message_contents`, `xmtp.mls.database`, `xmtp.device_sync.*`, `xmtp.identity.*` (about 33,800) | MLS payloads, local storage, device sync, identity. Not backend wire formats. |
| `crates/xmtp_mls_common` (11,397) | App-data registry, TLS codecs, group metadata, inbox id. |
| `crates/xmtp_archive` (1,153) | Archive format. |
| `crates/xmtp_mls/src/worker/device_sync/` (3,092) and the `history-server` docker service | Device sync stays. The history server is kept as a separate service for now. |
| `apps/db_tools`, `apps/error_glossary`, `apps/keepalive-probe` (retarget), `apps/xmtp_debug` (retarget) | Sections 10. |
| `crates/xmtp_mls/src/groups/app_data/migration.rs`, `crates/xmtp_mls_common/src/app_data/migration.rs` | App-data schema migration, not network migration. |
| `crates/xmtp_db/migrations/` | Clients start with a clean DB, but the chain still runs from empty. Delete only the cutover table migration. |
| `AuthCallback`, `AuthHandle`, `Credential` | Exposed through all 3 binding FFIs. Phase 6 reuses the shape. |
| `Cargo.toml:269-280` `hpke-rs` fork patch | Owner: leave it. |
| XIP-83 bidi client (section 3.1) | Backend-agnostic; the new backend implements XIP-83. |

`XmtpQuery` and `XmtpEnvelope` (`crates/xmtp_api_d14n/src/protocol/traits/xmtp_query.rs`)
are used by `crates/xmtp_api/src/xmtp_query.rs`, `crates/xmtp_mls/src/context.rs:23,107,201,278`,
and tests. Spec 002 decides whether the new client keeps this abstraction.

## 15. Cargo Dependencies

Each removal needs `cargo tree -i <crate>` confirmation. A dependency also used elsewhere stays.

### Drop with `apps/xnet` (Phase 0)

| Crate | Other users (grep of `Cargo.toml` files) | Verdict |
| --- | --- | --- |
| `bollard`, `bollard-stubs` | none | Drop |
| `serde_yaml` | none | Drop |
| `map-macro` | none | Drop |
| `ascii_table` | `crates/xmtp_db/Cargo.toml:22` (optional) | Stays |
| `dotenvy` | `apps/db_tools/Cargo.toml:18` | Stays |
| `bon` | `xmtp_db`, `xmtp_mls_common`, `xmtp_mls`; workspace `Cargo.toml:50` | Stays |
| `humantime` | `apps/keepalive-probe`, `apps/xmtp_debug` | Stays |
| `toml` | `crates/xmtp_db/Cargo.toml` | Stays |
| `toxiproxy_rust` | `xmtp_proto`, `xmtp_api_grpc`, `xmtp_common`, `xmtp_mls`, `bindings/node` | Stays |
| `clap-verbosity-flag`, `color-eyre` | `apps/xmtp_debug` and others | Stays |
| alloy features `signer-local`, `provider-http`, `network`, `rpc-types-eth`, `contract` | `crates/xmtp_id` dev and `test-utils` still enable `providers`, `rpc`, `network`, `provider-anvil-api`, `provider-anvil-node`, `contract` | Only the non-test alloy feature set shrinks |

### Drop with the validation shell (Phase 2)

| Crate | Note |
| --- | --- |
| `warp` (workspace `Cargo.toml:194`) | Only `apps/mls_validation_service/src/health_check.rs`. Drops unless the backend uses it (it should not). |
| `lru` (workspace `Cargo.toml:84`) | Moves to the shared crate with `cached_signature_verifier.rs`. |
| `vergen-gix` build dependency | Stays if the backend stamps a version. |

### Drop with the d14n arm and rename (Phase 3)

| Crate | Note |
| --- | --- |
| `impl-trait-for-tuples` | Only `crates/xmtp_api_d14n/Cargo.toml:22`. Drops. |
| `derive_builder` | Also `xmtp_db`, `xmtp_proto`, `xmtp_mls`. Stays. |
| `regex` | `D14N_MIGRATION_MSG_REGEX` goes; `xmtp_id` still uses it. Stays. |
| `arc-swap`, `pbjson-types`, `pin-project` | **verify** other users. |

### Proto move (Phase 1)

The `xmtp/proto` git clone in `crates/xmtp_proto/build.rs:72` goes. There is no Cargo
dependency on that repo; the clone happens at build time. `crates/xmtp_proto/proto_version`
goes with it.

### Add

Dev-only alloy node-bindings (`alloy/provider-anvil-node`) for in-process anvil in the SCW
tests. `crates/xmtp_id/Cargo.toml:81` already lists it under `test-utils`.

### Net expectation

`Cargo.lock` shrinks by the `bollard` Docker-client subtree and `warp`'s hyper stack. The alloy
provider and RPC subtree stays as a dev dependency. The expected reduction is smaller than
the earlier 10 to 18 percent estimate and must be verified with `cargo tree` before and after.

## 16. Unverified Items

Items the implementer must check. Do not treat them as decided.

1. `crates/xmtp_api_d14n/src/middleware/readonly_client.rs` (157): delete unless the new client keeps a read-only mode.
2. `crates/xmtp_proto/src/impls/update_dedupe.rs` (136): needed with one backend?
3. `crates/xmtp_api/src/scw_verifier.rs` (52): changes or dies when verification moves in-process.
4. `.github/workflows/lint-js.yml`: runs `just js bindings` for all JS SDKs; keep if the node SDK needs it.
5. `.github/workflows/release.yml:187,412`: cross-test gate comments; update with the release-gate change.
6. `apps/android/xmtpv3_example`: 0 Rust lines, excluded from the workspace; still used?
7. `arc-swap`, `pbjson-types`, `pin-project`: other users after the d14n arm goes.
8. `xmtp_api_grpc` `grpc_client/client.rs:341`: confirmed a test-only import of `PublishRequest`; remove in Phase 3 step 3.
9. `identity.api.v1` HTTP annotations: the generated Rust file does not reference the `grpc.gateway` types; the claim that the source `.proto` carries them comes from `docs/self-hosted/existing/proto.md` section 2.1, not from this checkout.
10. Phase line totals are approximate. The Phase 3 total (43,000) sums section 3, 4, 6, 7, and 11 and includes replaced code that leaves the tree; recount after step 3.
