# Phase 0: Tests Deleted for the Self-Hosted Transition

> **Ephemeral Phase 0 document.** This file records the test deletions made
> by the Phase 0 PR approved in <https://plan.ref.tools/ivo5pZxn63qnwJWA>.
> Delete this file when Phase 3 lands. It is not a long-lived design document.

This is the landed version of the scratch report that the plan reviewed. The
counts below match the commits in the PR. Where the scratch report was wrong,
the correct number is given here.

## 1. Method and rules

Candidates came from the test catalog in `docs/self-hosted/tests/`. Each
candidate was checked against the source file before deletion.

Rules:

1. **Delete only if the subject disappears.** A test goes when the type,
   endpoint, or service it exercises will not exist at project end.
2. **A `d14n` name is not proof.** `crates/xmtp_api_d14n` holds the v3
   client, the v3 endpoints, and the backend-agnostic XIP-83 core as well as
   the decentralization code. Only the d14n-specific modules were deleted.
3. **The crate must still compile.** Every fixture that a deleted test used
   was deleted too. No `#[allow(dead_code)]` was added.
4. **Delete a duplicate only when an equivalent test on a surviving surface
   proves the same requirement ID.** This applied once (section 3.3).
5. **Move, do not delete, MLS validation logic.** Not part of Phase 0.

Owner decisions on the open questions of the scratch report:

| Question | Decision |
| --- | --- |
| Q1: whole `apps/xnet`, or only its tests? | Delete the whole app. |
| Q2: wasm tests | Keep every wasm test. |
| Q3: `dev/release-tools` | Out of scope. Keep. |
| Q4: `apps/xmtp_debug` | Keep the app. Delete only its two backend-selection tests. |
| Q5: orphan device-sync files | Keep the files. Delete only their `mod tests` blocks. |

## 2. Summary

### 2.1 Per commit

| Commit | Area | Test functions deleted | In `just test v3` | In `just test d14n` |
| --- | --- | ---: | ---: | ---: |
| 1 | `xmtp_api_d14n` endpoint tests | 19 (18 nextest cases; one was `#[ignore]`) | yes | no |
| 2 | `xmtp_api_d14n` query, middleware, ordering tests | 49 (54 nextest cases) | yes | no |
| 3 | `xmtp_mls` d14n-only and migration tests | 9 | no | yes |
| 4 | `xmtp_mls` orphan device-sync test modules | 9 | never compiled | never compiled |
| 5 | `apps/xnet` and `apps/xmtp_debug` | 39 + 2 | not default members | not default members |
| **Total** | | **127** | | |

`just test v3` and `just test d14n` only build the workspace `default-members`
(`apps/mls_validation_service`, `bindings/*`, `crates/*`). `apps/xnet` and
`apps/xmtp_debug` were never part of those counts.

### 2.2 nextest counts, before and after

Measured with `cargo nextest list` on the untouched branch and on the PR head.

| Suite | Command | Before | After | Delta |
| --- | --- | ---: | ---: | ---: |
| v3 | `cargo nextest list --profile ci` | 2509 | 2437 | -72 |
| d14n | `cargo nextest list --features d14n --profile ci-d14n -E 'package(xmtp_mls)' -E 'rdeps(xmtp_mls)'` | 811 | 802 | -9 |

The scratch report said the d14n suite loses six tests. The correct number is
nine: four bidi tests, two migration tests, two `depends_on` tests, and the
network-severed registration test.

## 3. What was deleted

### 3.1 Commit 1: `xmtp_api_d14n` endpoint tests (19)

The `mod test` block was removed from each file. Production code is untouched.

| File | Tests | Req IDs |
| --- | ---: | --- |
| `crates/xmtp_api_d14n/src/endpoints/d14n/fetch_d14n_cutover.rs` | 2 | `API-REQ-010`, `API-REQ-016` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/get_inbox_ids.rs` | 2 | `API-REQ-010`, `API-REQ-016` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/get_newest_envelopes.rs` | 2 | `API-REQ-010`, `API-REQ-016` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/get_nodes.rs` | 3 | `API-REQ-010`, `API-REQ-020` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/health_check.rs` | 1 | `API-REQ-020` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs` | 2 | `API-REQ-010`, `API-REQ-017` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/query_envelopes.rs` | 3 | `API-REQ-010`, `API-REQ-018` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_topics.rs` | 4 | `API-REQ-010`, `API-REQ-019` |

### 3.2 Commit 2: `xmtp_api_d14n` query, middleware, and ordering tests (49)

The scratch report counted 34 + 13 = 47 rows for this group. The correct
count is 49 test functions. Two of them are `rstest` tests with three and two
cases, so nextest counts 54.

| File | Tests | Req IDs |
| --- | ---: | --- |
| `crates/xmtp_api_d14n/src/queries/d14n/connection.rs` | 6 | `API-REQ-048` |
| `crates/xmtp_api_d14n/src/queries/d14n/identity.rs` | 1 | `API-REQ-047` |
| `crates/xmtp_api_d14n/src/queries/d14n/mls.rs` | 3 | `API-REQ-049`, `API-REQ-050` |
| `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs` | 6 (9 cases) | `API-REQ-053` |
| `crates/xmtp_api_d14n/src/queries/client_bundle.rs` | 1 | `API-REQ-054` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` (file removed) | 14 | `API-REQ-043`, `API-REQ-044` |
| `crates/xmtp_api_d14n/src/middleware/multi_node_client/client.rs` | 4 | `API-REQ-023` |
| `crates/xmtp_api_d14n/src/middleware/multi_node_client/gateway_api.rs` | 3 | `API-REQ-024` |
| `crates/xmtp_api_d14n/src/protocol/in_memory_cursor_store.rs` | 5 | `API-REQ-038` |
| `crates/xmtp_api_d14n/src/protocol/sort/causal.rs` | 2 | `API-REQ-039` |
| `crates/xmtp_api_d14n/src/protocol/sort/timestamp.rs` | 1 | `API-REQ-042` |
| `crates/xmtp_api_d14n/src/protocol/order.rs` | 2 | `API-REQ-040` |
| `crates/xmtp_api_d14n/src/protocol/resolve/network_backoff.rs` | 3 | `API-REQ-041` |

Fixtures deleted because only those tests used them:

- `crates/xmtp_api_d14n/src/queries/d14n/test.rs` and
  `crates/xmtp_api_d14n/src/queries/d14n/test/send_group_message.rs`
- `crates/xmtp_api_d14n/src/protocol/utils/test/dependency_resolution_test.rs`
- `depends_on_one` in `crates/xmtp_api_d14n/src/protocol/utils/test/props.rs`
- `has_dependency_on_any`, `only_depends_on`, and `From<&OrphanedEnvelope>` in
  `crates/xmtp_api_d14n/src/protocol/utils/test/test_envelope.rs`
- `InMemoryCursorStore::orphan_count`. The `icebox` helper stays because
  `crates/xmtp_api_d14n/src/queries/stream/ordered.rs` still uses it. The
  scratch report said both helpers become dead; that was wrong.

Proptest regression seeds deleted with their owning test:

- `crates/xmtp_api_d14n/proptest-regressions/protocol/order.txt`
- `crates/xmtp_api_d14n/proptest-regressions/protocol/sort/causal.txt`
- `crates/xmtp_api_d14n/proptest-regressions/protocol/sort/timestamp.txt`
- `crates/xmtp_api_d14n/proptest-regressions/queries/d14n/mls.txt`

Dev-dependencies: `proptest`, `mockall`, and `ctor` stay. Surviving tests
still use all three.

### 3.3 Commit 3: `xmtp_mls` d14n-only and migration tests (9)

| File | Tests | Req IDs | Coverage after deletion |
| --- | ---: | --- | --- |
| `crates/xmtp_mls/src/subscriptions/d14n_bidi_tests.rs` (file removed) | 4 | `MLS-REQ-096` to `MLS-REQ-099` | **No loss.** `crates/xmtp_mls/src/subscriptions/bidi_tests.rs` proves the same four IDs on the v3 wire, 1:1. |
| `crates/xmtp_mls/src/migration_tests.rs` (file removed) | 2 | `MLS-REQ-041` | None needed. Clients start with a clean DB; there is no live migration. |
| `crates/xmtp_mls/src/groups/tests/test_message_dependencies.rs` (file removed) | 2 | (d14n `depends_on`) | None needed. `depends_on` has no successor. |
| `crates/xmtp_mls/src/registration_visible/tests.rs`: `test_wait_for_registration_visible_fails_when_network_severed` | 1 | (registration visibility) | **Gap.** The success path stays covered by `test_wait_for_registration_visible_after_registration`. Follow-up: write a network-severed registration test against the new backend in Phase 3. |

One assertion was ported from the d14n bidi tests into `bidi_tests.rs`: in
`bidi_history_only_catches_up_then_delivers_nothing_live`, a stream that ends
without a client half-close is now a failure (XIP-83 server requirement 9).
Before, `bidi_tests.rs` accepted it.

Also deleted, because only `migration_tests.rs` used them:
`ClientBuilder::local_migration` in `crates/xmtp_mls/src/utils/test/mod.rs`,
and the `MigrationTestClient`, `MigrationXmtpMlsContext`,
`MigrationXmtpClient`, and `LocalOnlyMigrationClientCreator` aliases in
`crates/xmtp_mls/src/utils/test/definitions.rs`. The `ci-d14n` filter in
`.config/nextest.toml` no longer names the deleted migration test.

### 3.4 Commit 4: orphan device-sync test modules (9)

`crates/xmtp_mls/src/worker/device_sync/mod.rs` does not declare
`message_sync` or `consent_sync`, so these tests never compiled or ran.

| File | Tests | Req IDs |
| --- | ---: | --- |
| `crates/xmtp_mls/src/worker/device_sync/message_sync.rs` | 8 | `MLS-REQ-063` to `MLS-REQ-067` |
| `crates/xmtp_mls/src/worker/device_sync/consent_sync.rs` | 1 | `MLS-REQ-076` |

Only the `mod tests` blocks were deleted. The files and their `impl` blocks
stay. Follow-up: remove both dead modules in a separate change.

### 3.5 Commit 5: `apps/xnet` and `apps/xmtp_debug` (41)

`apps/xnet` was deleted in full (39 tests). It is an orchestration tool for
xmtpd clusters. Every reference went with it: the root `Cargo.toml` member,
`exclude` entries, and workspace dependencies; `.config/hakari.toml`
exclusions; `flake.nix`, `nix/apps.nix`, `nix/package/xnet-cli.nix`,
`nix/lib/filesets.nix`, `nix/package/mls_validation_service.nix`; and
`dev/docker/xnet.toml`. `xnet-lib` declared its own crate versions for
`bollard`, `bollard-stubs`, `ascii_table`, `serde_yaml`, `map-macro`,
`dotenvy`, and `bon`, so no workspace dependency became unused.

| File | Tests | Req IDs |
| --- | ---: | --- |
| `apps/xnet/lib/src/config/toml_config_test.rs` | 17 | `RUST-REQ-028`, `RUST-REQ-036` |
| `apps/xnet/lib/src/services/traefik_config.rs` | 8 | `RUST-REQ-033` |
| `apps/xnet/lib/src/node_provisioner.rs` | 5 | `RUST-REQ-032` |
| `apps/xnet/lib/src/config/address_mode.rs` | 4 | `RUST-REQ-027` |
| `apps/xnet/lib/src/types.rs` | 4 | (xnet types) |
| `apps/xnet/lib/src/wallet_funding.rs` | 1 | `RUST-REQ-037` |
| `apps/xmtp_debug/src/args.rs`: `perf_with_d14n_and_backend_is_valid`, `explicit_gateway_url_overrides_perf` | 2 | (xdbg backend args) |

The other 43 `xmtp_debug` tests stay.

## 4. Not deleted in Phase 0

These are d14n or v4 code but are not safe to delete yet, or are out of scope.

| Path | Tests | Why not now |
| --- | ---: | --- |
| `crates/xmtp_api_d14n/src/protocol/impls/protocol_envelopes.rs`, `protocol/traits/visitor.rs`, `protocol/macros.rs` | 8 | The `EnvelopeVisitor` framework is the shared decode path. Phase 3. |
| `crates/xmtp_api_d14n/src/protocol/extractors/*.rs` | 29 | `TopicExtractor` is used by the live v3 path. Phase 3. |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs`, `queries/bidi.rs`, `queries/bidi_transport_props.rs` | 65 | **Keep.** Backend-agnostic XIP-83 core. The new backend keeps XIP-83. |
| `crates/xmtp_api_d14n/src/endpoints/v3/**`, `queries/v3/**` | 48 | v3 wire, still live until Phase 3. The `queries/v3/connection.rs` tests define the XIP-83 connection contract; port them, do not drop them. |
| `crates/xmtp_api_d14n/src/middleware/readonly_client.rs`, `middleware/read_write_client/client.rs` | 4 | `ReadWriteClient` dies with the payer service. Phase 3. |
| `crates/xmtp_proto/src/types/global_cursor.rs::dominates_empty` | 1 | `GlobalCursor` is still referenced. Delete with the type in Phase 3. |
| `apps/mls_validation_service/src/handlers.rs`, `cached_signature_verifier.rs` | 10 | Move eight into the shared validation crate; delete the two `tonic::Status` tests when the binary goes. Phase 2. |
| Cross-SDK duplicates under `SHARED-*` IDs | many | Each row asserts a platform API shape. Reduce in a dedicated PR at the end of Phase 3. |
| `dev/release-tools/tests/**` | 188 | Out of scope. SDK release automation. |
| All wasm, bindings, and SDK tests | ~1,500 | Phase 3 rewrites the harnesses. Delete after, not before. |

## 5. Explicit keep list

Recorded so a later change does not delete these by name.

| Path | Why it survives |
| --- | --- |
| `crates/xmtp_api_grpc/src/**` | Generic gRPC transport primitives. The new backend also speaks gRPC. |
| `crates/xmtp_api/src/mls.rs`, `crates/xmtp_api/src/identity.rs` | `ApiClientWrapper` logic through a mockable trait: retry, paging, batching, rate limits. |
| `crates/xmtp_api_d14n/src/middleware/auth.rs` | Generic auth-token middleware. Phase 6 needs it. |
| `crates/xmtp_api_d14n/src/queries/stream/extractor.rs`, `queries/stream/ordered.rs` | Generic stream combinators. |
| `crates/xmtp_api_d14n/src/protocol/utils/test/{props,test_envelope}.rs` | `ordered.rs` still uses `missing_dependencies` and `TestEnvelope`. |
| `apps/db_tools/src/tasks/**` | Client SQLite migration tooling. The client DB survives. |
| `apps/keepalive-probe/src/main.rs` | Pure helper functions. Useful against the new backend. |
| `apps/xmtp_debug/**` except the two `args.rs` tests | Generic DAG, result-counting, and store logic. |
| `crates/xmtp_proto/src/types/cursor.rs` | `Cursor` constructors are used by the live v3 path. |

## 6. Follow-ups

1. Phase 3: write a network-severed registration test against the new
   backend to replace
   `test_wait_for_registration_visible_fails_when_network_severed`.
2. Remove the orphan modules
   `crates/xmtp_mls/src/worker/device_sync/message_sync.rs` and
   `consent_sync.rs` in full.
3. Update `docs/self-hosted/tests/existing-tests/*.md` and
   `existing-requirements.md` for the deleted rows when those catalog files
   land on this branch. They were not in the tree when this PR was made.
