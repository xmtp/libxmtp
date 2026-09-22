# XMTP Self-Hosted Transition

XMTP is replacing both the v3 network (`xmtp-node-go`) and the v4 network (`xmtpd`) with one self-hosted backend. Phases 0 through 3 are complete, as are 4.1, 4.3, 4.5, 4.7, and 5.1.

This document is now mostly a historical record of scope and sequencing. It is not agent context. The approved specs in `docs/specs` are authoritative for current behavior.

## Compatibility

- There is no expectation of live message migration, or of perfect compatibility at the wire level, with either existing version.
- Compatibility is expected at the client SDK level. With minimal setup changes (a backend URL instead of `env`, and new client authentication) an application built on the existing SDKs should be able to connect to a self-hosted backend without changing application code. The goal is to minimize changes that touch application code.
- Those clients are expected to start with a clean local database the first time they connect to a self-hosted backend.

## Outcome

- A new backend, `apps/backend`, written in Rust. It takes the best parts of `xmtpd` and `xmtp-node-go` and implements the bare minimum API surface an XMTP client needs to exercise the core SDK functionality: registering identities, publishing envelopes, querying envelopes, and efficiently streaming envelopes. It should be a single binary that can be horizontally scaled and load balanced, with no local state.
- `libxmtp` and the platform SDKs are overhauled to work with this backend exclusively. Dead code for decentralization, blockchains, the payer service, and originator IDs is removed. Smart-contract-wallet (SCW) signature verification remains a chain-RPC dependency for SDK compatibility. The complexity `xmtpd` and v4 added for ordering messages between originators is removed. Streaming is expected to be simplified.
- The `xmtpd`, `xmtp-node-go`, and `proto` repositories are deprecated. The entire stack lives in `libxmtp`. All `.proto` files live in a `proto/` folder in this repository.
- Server-based history transfer is removed. Message-based device sync and file-based archive export and import stay.
- Durable backend state requires PostgreSQL 17 or later. Stream queues, subscription state, and reconstructible caches may live in memory. A reconnect to another instance must not need the previous instance's state.
- The MLS validation service stops being a standalone service and becomes a crate the new backend uses.
- The payer service and all related code, and `xmtp_api_d14n` and associated code, are removed from `libxmtp`.
- All code shared by the backend and the client lives in a crate separate from `apps/backend` and `crates/xmtp_mls`.
- Every important behavior is tested once. Redundant tests are avoided or removed. Integration tests cover all realistic client use cases: happy path, edge cases, and all failure modes.
- Simplify, simplify, simplify. The result should be a substantial net reduction in complexity: fewer variations, fewer cases, fewer things to worry about.

## Ways of working

### Specs and plans

- Specs, once approved, live in `docs/specs`. Ref may be used to get approval on draft specs. Specs are long-lived documents for human and AI readers. They are high level and describe business rules, behaviors, and error cases. They do not reference files or modules. They may include code snippets, pseudocode, or type definitions where the code is a critical API surface unlikely to change.
- Plans live in Ref. Plans are short-lived, reviewed by a human before implementation, and mostly worked with by agents. They are a specific project plan and may reference files, modules, and lines. They match the `/writing-specs` skill format and include specific EARS requirements that must be satisfied.
- Not all work needs a spec. All implementation work needs a plan.
- Spec 002 is written in Phase 1, before the `xmtp_mls` audit, and approved before Phase 2.
- Spec 004 must be approved before Phase 2 streaming implementation. Spec 003 first records backend admission and identity trust limits; its client MLS security description is completed during Phase 3.

Expected specs by the end of the project:

| Spec | Content |
| --- | --- |
| `001_backend_api.md` | Public API of the backend |
| `002_backend_architecture.md` | Backend service design and database schema |
| `003_message_security.md` | Backend and identity trust limits, then adaptation of the `xmtp_mls` README |
| `004_streaming.md` | Streaming APIs and semantics |
| `005_push_subscriptions.md` | Push recipients, subscriptions, dispatch, and the client sync task |

### Git

- The base and integration branch is `origin/self-hosted`. Each phase results in one pull request to `self-hosted`, or, for very large phases, a `gh stack` of pull requests that merge into `self-hosted`.
- No other work happens in the impacted repositories during this project. Planning done ahead of time must be treated as authoritative by future agents.
- Git worktrees may be used for parallelism, sparingly, and only for truly disjoint work with no dependency on other work.
- Given the scope, commits and entire phases may leave parts of the codebase with failing tests that cannot run. `cargo build` should work at the end of every phase.

### Guidelines

`docs/self-hosted/agent-context.md` is the standing document for implementer agents. It carries the hard rules for this project - which must be followed and which may be broken - and the architecture constraints. Because of the project's scope it may contradict otherwise good advice, for example leaving a branch with failing CI or deleting existing functionality. Agents read it once at the start of a session. It is deleted when the project ends. The macros, utilities, and coding practices idiomatic in this repository live in the skills under `.agents/skills/` and outlive the project.

### Phase 0 documents

Ephemeral documents produced in Phase 0 live in `docs/self-hosted`, including the existing-behavior wiki in `existing/`. They are deleted when the project ends.

## Phases

### Phase 0: Preparation

Status: complete.

- Dispatch sub-agents to research the existing implementations in `libxmtp`, `xmtp-node-go`, `xmtpd`, and `proto`, and catalog all current behaviors and requirements of the existing endpoints in a detailed wiki at `docs/self-hosted/existing`. Required content: the input parameters of each endpoint and their exact formats (serialization, bindings of fields to database tables), the database schema, what conditions trigger errors and how errors are surfaced to the client, limits applied to endpoints, rate limiting, and anything else relevant to future implementers. All claims cite function names and file paths. The goal is a complete and accurate specification of the relevant parts of the existing services.
- Interrogate the proposed `proto/backend/v1/backend.proto`. Will it lead to a performant backend that can handle all needs of the new client? Analyze the expected callers of each backend API in `libxmtp` and ensure their core business requirements can be met.
- Look for macros, utilities, and coding practices that are idiomatic in this repository. Record them for implementer agents.
- Review `libxmtp` and determine what code can be removed by the end of the project: a concrete list of deletions, and the downstream change of each.
- Refine this document: shorter, tighter, internally consistent.
- Audit all `AGENTS.md` and `CLAUDE.md` files. They must be up to date with the code at project start and extremely concise. Prefer `AGENTS.md` over `CLAUDE.md`; each `AGENTS.md` has a sibling `CLAUDE.md` pointer. Each package, crate, and app involved in this project has an `AGENTS.md` with the basic commands to build, check, and test the package, and to test a single file or function. Language can be borderline caveman.
- Write the hard rules for implementer agents.
- Using the Phase 0 test report, take a first pass at tests that will not be needed at the end of the project, and create a pull request that deletes them now. The same pull request deletes `apps/xnet` and its Nix references.

Expected pull requests: a stack of two, one for documentation changes and one for test deletions. They can be worked on in parallel worktrees.

### Phase 1: Scaffolding

Status: complete.

- Scaffolded `apps/backend` with a Nix build, a Docker image, and CI checks.
- Created `crates/xmtp_mls_validation` for shared payload parsing, validation,
  topic derivation, and canonical envelope encoding, with portable fixtures that
  build and test on native and wasm without client database dependencies.
- Moved the code the backend shares out of `crates/xmtp_mls` into the shared
  crates, guided by spec 002.
- Made `proto/` authoritative for all `.proto` files, added Buf linting, and
  removed the old generated tree and the `proto` repository dependency.

### Phase 2: Backend

Status: complete. Specs 001, 002, and 004 describe the result.

- The full `proto/backend/v1/backend.proto` API surface, plus the standard gRPC
  health service. No version or metadata endpoint in v1.
- Single-client bidirectional streaming with one ingestion cursor per topic,
  atomic interest updates, and fixed catch-up targets (spec 004). Static
  gRPC-Web subscriptions give browsers the same ordered feed.
- A PostgreSQL schema indexed for every query parameter, and read replica
  support. Publish and Query use the primary; newest reads, streams, and
  identity lookups may use a replica.
- One stateless binary that scales horizontally. The MLS validation service is
  gone; the backend calls the shared validation crate directly.
- A TOML config format with a published schema. Config files may reference
  environment variables for secrets.
- No caller quotas or authentication in this phase. Per-stream Update and client
  Ping token buckets protect the stream protocol (10 frames/s each, burst 100).

### Phase 3: Client Support And Cleanup

Status: complete. Delivered as a four-pull-request stack (#4075 to #4078).

- `xmtp_mls` talks only to the self-hosted backend. A backend URL is a required
  client option with no default. `env` is now a string used only for information
  and for choosing the database file name when no `dbPath` is given.
- `historySyncUrl` and all server-based history sync are removed. Message-based
  device sync and file-based archive export and import stay.
- Native streams use the bidi router over `Subscribe`. WebAssembly uses the
  legacy stream stack over `SubscribeStatic`.
- `xmtp_api` is renamed `xmtp_api_backend` and supports only the new backend.
  Dead v3, d14n, and payer code is gone. The auth and read-only middleware stay.
- The spec 001 client obligations are implemented: keyed key-package results
  with absence, batch chunking, identity and commit-log paging, static
  subscription splitting, and status-based retry classification. Public SDK
  methods and stream callbacks are preserved.
- Every binding in `bindings/`, every SDK in `sdks/`, and `apps/xmtp_debug` are
  updated. Dead dev scripts and justfile entries are removed.
- `dev/docker/compose.yml` now holds `db` (PostgreSQL 18), `replica`, `backend`,
  `anvil`, `toxiproxy`, `tempo`, `prometheus`, and `grafana`. The legacy node,
  validation, and history services and the d14n compose file are gone.

### Phase 4: Polish

#### 4.1: Docs Site

Status: complete.

- A Starlight docs site in `apps/docs`, built and linted from this repository.
  All Markdown and site code live here.
- Content is a simplified subset of `xmtp/docs-xmtp-org`, plus generated API
  references for the SDKs and the Agent SDK.
- Deployment guides for Fly.io, AWS ECS Fargate, Kubernetes with Helm, and
  Railway, each validated locally, with a reusable HAProxy TLS terminator.

#### 4.2: Message Pruning

- Add deletion of expired rows from the database as a job that runs hourly (with a lock to prevent overlap). Ensure query is fast and indexed. Every row carries an expiry set at publish time. Group application messages, welcomes, and key packages use 90 days (the fixed duration called 3 months). Identity updates, commit-log entries, and group messages marked as commits or proposals never expire.
- Before this phase, expiry is metadata only: no pruning. Define newest-watermark behavior and the interaction between pruning and the tailer/streams.

#### 4.3: Authentication

Status: complete. Delivered as a three-pull-request stack (#4092 to #4094).

- Backend JWT verification, configured with approved public keys or JWKS URLs,
  audiences, and required scopes. When authentication is required, requests with
  a missing or invalid token are rejected.
- Clients attach caller tokens to every gRPC request through the auth
  middleware, refetch credentials on unauthorized responses, and back off into a
  lockout instead of retrying forever.
- Added `EphemeralBackend` so `xmtp_mls` tests can run against a backend with a
  specific configuration.

#### 4.4: Rate Limiting

- Add support for rate-limiting using an in-memory token bucket rate limiter. Create a mapping of rate limit costs to request types, such that each request (or mutation of a bidi stream) consumes a certain number of tokens. If authentication is enabled, user identifier for rate limiting is the `sub` claim from the JWT. If auth is disabled, use the client IP. Reject requests that exceed rate limits.

#### 4.5: Metrics And Telemetry

Status: complete (#4081). See [backend observability](../backend-observability.md)
for configuration, metrics, traces, alerts, and the end-to-end check.

- Full OpenTelemetry traces and Prometheus metrics for the backend, reusing
  metric names, labels, and conventions from `xmtpd` and `xmtp-node-go` where
  they applied.

#### 4.6: Benchmarks And Performance

- Backend benchmarks for all core database operations, including runs against a database preloaded with 100k, 1M, and 10M messages. Ensure all supported queries use database indexes and return quickly. Ensure benchmarks are run with a good diversity of queries (small topic list, large topic list, high cursors, low cursors). Prepare a detailed benchmark report.
- Attempt to optimize the schema for both read and write performance: how to unlock parallel writes without breaking total ordering per topic, how to reduce index size for common queries, how to use indexes more efficiently for the most important queries, and whether safe, low-maintenance partitioning can serve the common pattern (most queries read relatively new messages). Test changes against benchmarks.

#### 4.7: Push Subscriptions

Status: complete. Spec `005_push_subscriptions.md` (since deleted; see git
history), approved 2026-09-14. Delivered as a five-pull-request stack (#4119 to #4124).

- Backend registration API and storage for push recipients and subscriptions.
- A dispatcher that delivers signed HTTPS webhooks, plus direct APNs and FCM
  delivery with bounded credential handling.
- Client notification sync, bindings, and SDK notification APIs. The legacy push
  path is removed.

#### 4.8: Self Publishing SDK Versions

- If someone forks this repo, how do they release their own version of the Node, Android, iOS, or Browser SDKs in a way that they can use in their own app.
- Will need to test this end-to-end across all SDKs
- Update documentation with guides for how to do this across all languages

#### 4.9: Integration Test Suite

- Create a more aggressive fork/integration test suite that can catch real-world issues and runs in CI
- Adapted from the old fork suite

### Phase 5: Extras

#### 5.1: Overhaul Message Fetching

Status: complete. Contracts in <https://plan.ref.tools/W6p6z0HV0nVruZwI>
and #4086. Implementation in #4088, #4089, and #4096.

- Durable client receipt and state processing over the new backend streaming
  APIs, with a Rust reader and focused regression tests.
- The lease ledger carries durable progress, so a reopen after a wire failure
  resumes instead of replaying from each lease's floor.
- A drop reason crosses the lease boundary, so transport backpressure no longer
  fires `on_close`. That callback now means an explicit close or a
  non-retryable failure, as P3-STR-015 intended.

#### 5.2: Codegen SDKs

- Hollow out our iOS/Android/Browser/Node SDKs to be primarily codegen driven via Uniffi. Goal is a 90% reduction in lines of code.
- Goal here is to dramatically simplify making SDK changes, and to make the release process much much easier.

### Deferred test and tool fixes

- The keepalive probe always starts TLS in non-subscription mode. That mode cannot connect to a plaintext backend. The subscription mode honors the URL scheme.
- Investigate the iOS `testCanStreamAndUpdateNameWithoutForkingGroup` flake when tests run with `--parallel`.
- Pin the iOS health-probe package to the flake input. The same-repository pull-request gate is already in place.
