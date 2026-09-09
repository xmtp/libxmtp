# XMTP Self-Hosted Transition

XMTP is replacing both the v3 network (`xmtp-node-go`) and the v4 network (`xmtpd`) with one self-hosted backend. The backend and client integration are complete through Phase 3. This document is the plan of record for the project. Future agents must treat it, and the approved specs in `docs/specs`, as authoritative.

## Compatibility

- There is no expectation of live message migration, or of perfect compatibility at the wire level, with either existing version.
- Compatibility is expected at the client SDK level. With minimal setup changes (a backend URL instead of `env`, and new client authentication) an application built on the existing SDKs should be able to connect to a self-hosted backend without changing application code. The goal is to minimize changes that touch application code.
- Those clients are expected to start with a clean local database the first time they connect to a self-hosted backend.

## Outcome

- A new backend, `apps/backend`, written in Rust. It takes the best parts of `xmtpd` and `xmtp-node-go` and implements the bare minimum API surface an XMTP client needs to exercise the core SDK functionality: registering identities, publishing envelopes, querying envelopes, and efficiently streaming envelopes. It should be a single binary that can be horizontally scaled and load balanced, with no local state.
- `libxmtp` and the platform SDKs are overhauled to work with this backend exclusively. Dead code for decentralization, blockchains, the payer service, and originator IDs is removed. Smart-contract-wallet (SCW) signature verification remains a chain-RPC dependency for SDK compatibility. The complexity `xmtpd` and v4 added for ordering messages between originators is removed. Streaming is expected to be simplified.
- The `xmtpd`, `xmtp-node-go`, and `proto` repositories are deprecated. The entire stack lives in `libxmtp`. All `.proto` files live in a `proto/` folder in this repository.
- Server-based history transfer is removed. Message-based device sync and file-based archive export and import stay.
- Durable backend state lives in Postgres. Stream queues, subscription state, and reconstructible caches may live in memory. A reconnect to another instance must not need the previous instance's state.
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

### Git

- The base and integration branch is `origin/self-hosted`. Each phase results in one pull request to `self-hosted`, or, for very large phases, a `gh stack` of pull requests that merge into `self-hosted`.
- No other work happens in the impacted repositories during this project. Planning done ahead of time must be treated as authoritative by future agents.
- Git worktrees may be used for parallelism, sparingly, and only for truly disjoint work with no dependency on other work.
- Given the scope, commits and entire phases may leave parts of the codebase with failing tests that cannot run. `cargo build` should work at the end of every phase.

### Guidelines

`docs/self-hosted/guidelines.md` is an ephemeral set of hard rules for implementer agents: which rules must be followed and which may be broken. Target under 50 lines and 750 tokens. Every word must earn its place and be unambiguous. Because of the project's scope, it may contradict otherwise good advice (for example: leaving a branch with failing CI, or deleting existing functionality). Example rules: common functions and utilities between crates live in a shared crate or module; the backend is a single binary that can be horizontally scaled and load balanced, with no local state.

`docs/self-hosted/style-guide.md` covers the macros, utilities, and coding practices idiomatic in this repository and encourages use of pre-existing utilities.

### Phase 0 documents

Ephemeral documents produced in Phase 0 live in `docs/self-hosted`: the existing-behavior wiki in `existing/`, the deletion inventory in `deletions.md`, the test catalogue in `tests/`, and the retirement record in `tests/retired-requirement-ids.md`. They are deleted when the project ends.

## Phases

### Phase 0: Preparation

- Dispatch sub-agents to research the existing implementations in `libxmtp`, `xmtp-node-go`, `xmtpd`, and `proto`, and catalog all current behaviors and requirements of the existing endpoints in a detailed wiki at `docs/self-hosted/existing`. Required content: the input parameters of each endpoint and their exact formats (serialization, bindings of fields to database tables), the database schema, what conditions trigger errors and how errors are surfaced to the client, limits applied to endpoints, rate limiting, and anything else relevant to future implementers. All claims cite function names and file paths. The goal is a complete and accurate specification of the relevant parts of the existing services.
- Interrogate the proposed `proto/backend/v1/backend.proto`. Will it lead to a performant backend that can handle all needs of the new client? Analyze the expected callers of each backend API in `libxmtp` and ensure their core business requirements can be met.
- Look for macros, utilities, and coding practices that are idiomatic in this repository. Create `docs/self-hosted/style-guide.md`.
- Review `libxmtp` and determine what code can be removed by the end of the project: a concrete list of deletions, and the downstream change of each.
- Refine this document: shorter, tighter, internally consistent.
- Audit all `AGENTS.md` and `CLAUDE.md` files. They must be up to date with the code at project start and extremely concise. Prefer `AGENTS.md` over `CLAUDE.md`; each `AGENTS.md` has a sibling `CLAUDE.md` pointer. Each package, crate, and app involved in this project has an `AGENTS.md` with the basic commands to build, check, and test the package, and to test a single file or function. Language can be borderline caveman.
- Write `docs/self-hosted/guidelines.md`.
- Using the test report in `docs/self-hosted/tests/`, take a first pass at tests that will not be needed at the end of the project, and create a pull request that deletes them now. The same pull request deletes `apps/xnet` and its Nix references.

Expected pull requests: a stack of two, one for documentation changes and one for test deletions. They can be worked on in parallel worktrees.

### Phase 1: Scaffolding

- Scaffold `apps/backend`. Ensure it builds with Nix. Give it a hello-world main and a single test.
- Scaffold `crates/xmtp_mls_validation` for shared payload parsing, validation, topic derivation, and canonical envelope encoding. Include its `test-utils` fixtures. Ensure it builds and tests independently on native and wasm, without client database dependencies or accidental workspace feature unification.
- Audit all of `crates/xmtp_mls`, including its runtime and test utilities, against the expected scope of the backend API. Move every function, struct, utility, and type the backend will share out of `xmtp_mls` and into the appropriate other crate. Spec 002 must be written before this audit; without it the comparison cannot be accurate.
- Extract the shared validation logic and fixtures in this phase. Phase 2 connects them to backend storage and requests. Share canonical envelope encoding and hashing as well as topic derivation; preserve the separate client MLS message-ID and payload-hash rules.
- Ensure the backend can produce a Docker image, the way the MLS validation service is built with Nix. Ensure all check, build, and test commands work and maximize Nix caching.
- Create the `proto/` folder for all `.proto` files. Copy every required file from the `proto` repository (including files for endpoints this project removes, such as v4) plus the new backend protos. Set up Buf linting in the justfile. Update all scripts and `crates/xmtp_proto` to make this folder authoritative, and delete the old generated tree and the `proto` repository dependency in the same phase.
- Ensure tests for the new crates run in CI.
- Temporarily disable these GitHub Actions to speed up CI: anything for xdbg, wasm, the browser SDK, cross-test, and `nightly-protos.yml`. The browser SDK is handled in a later phase.

### Phase 2: Backend

Specs 001 and 002 must be completed and approved before this phase begins. This phase sets up the backend, creates its tests, and implements the complete API surface. The backend is not integrated into any client or SDK yet, except for minimal stateless test harnesses required by the test suite.

- Single-client bidirectional streaming with one ingestion cursor per topic, atomic interest updates, and fixed catch-up targets (spec 004). This replaces XIP-83. Static gRPC-Web subscriptions provide the same ordered feed for browsers.
- Complete support for the API surface defined in `proto/backend/v1/backend.proto`. The standard gRPC health service is served. There is no version or metadata endpoint in v1.
- A Postgres schema designed for the API surface, with indexes for every query parameter.
- A single binary that can be horizontally scaled and load balanced. The MLS validation service is not used; the backend connects storage to the shared validation logic extracted in Phase 1.
- Support read replicas from day one. Each configured replica URL points to one replica instance. Publish and Query use the primary; newest reads, streams, and identity lookups may use the replica.
- No caller quotas, authentication, or authorization. Phase 6 adds them. Exception: per-stream Update and client Ping token buckets protect the stream protocol in Phase 2 (10 frames/s each, burst 100).
- Establish, and include in the spec, a concise TOML config format for all server configuration. Config files may reference environment variables for secrets. The format should have a defined schema that can be publicly hosted and referenced by config files that support Taplo schemas.

### Phase 3: Client Support And Cleanup

Status: implementation complete. Tasks 1 to 14 implement the backend transition. The identifier sweep passes with documented historical and archive-format exceptions. The five coverage gaps remain explicit in `tests/existing-tests.md`. Full CI verification is still required. The follow-ups below remain outside Phase 3.

Replace all backend selection in `xmtp_mls` with the self-hosted backend. This requires updates to every binding in `bindings/`, every SDK in `sdks/`, and the CLIs in `apps/`. The diff is large and changes the test harness of every client SDK. `docs/self-hosted/deletions.md` gives the order of the deletions in this phase.

- Update client creation options, removing anything to do with d14n or other deprecated/removed features. A backend URL is a new required config option with no default. `env` should transition to a String, and would only be used informationally and for selecting database file name if no explicit `dbPath` was specified.
- Remove `historySyncUrl` from all client configuration options, and any downstream support for the history sync server. We still want device sync that is message-based, or file-based restores, but we don't need any server-based history sync. It's mostly gone already anyways.
- Native streams use the bidi router over backend `Subscribe`. WebAssembly uses the legacy stream stack over `SubscribeStatic`. The backend implements the spec 004 wire contract. Shared client ingestion and processing-based catch-up are deferred to Phase 5.1.
- Update API clients (`xmtp_api`, renamed `xmtp_api_backend`) to exclusively support the new backend. Remove all dead proto code, and all dead code related to v3 or d14n. Keep the auth and read-only middleware. Remove the payer read-write middleware. We will be using and extending the auth middleware in a later phase.
- Implement the spec 001 client obligations: keyed key-package results with absence, batch chunking, identity and commit-log query paging, static-subscription splitting, and status-based retry classification. Anything else required to make the client tests pass against a self-hosted backend and preserve correct behavior. Preserve public SDK methods and stream callbacks.
- Preserve canonical envelope bytes for publish retries and hash matching. An oversized publish response can follow a committed write; response failure does not prove rollback.
- `apps/xmtp_debug` stays as an app. Its backend selection and other dead functionality are deleted as the code they depend on goes.
- The `anvil` service stays. The SCW verifier tests keep using it, as the owner decided on 2026-09-08.
- Audit all scripts in the dev folder and justfile and remove any scripts or configuration that is now dead code.
- The `anvil` service stays in `dev/docker/docker-compose.yml`. The stack contains `db` (Postgres 18), `backend`, `anvil`, and `toxiproxy`. The legacy node, validation, and history services and the separate d14n compose file are removed.

### Phase 4: Polish

#### 4.1: Docs Site

- A new docs site backed entirely by this repo.
- Would take a subset of the docs from the existing <https://github.com/xmtp/docs-xmtp-org>. Hopefully greatly simplified for easier maintenance
- All markdown files and docs code lives inside this repo

#### 4.2: Message Pruning

- Add deletion of expired rows from the database as a job that runs hourly (with a lock to prevent overlap). Ensure query is fast and indexed. Every row carries an expiry set at publish time. Group application messages, welcomes, and key packages use 90 days (the fixed duration called 3 months). Identity updates, commit-log entries, and group messages marked as commits or proposals never expire.
- Before this phase, expiry is metadata only: no pruning. Define newest-watermark behavior and the interaction between pruning and the tailer/streams.

#### 4.3: Authentication

- Provide backend configuration options for JWT authentication. Allow configuration of approved public keys or JWKs URLs, audiences, and required scopes. If authentication is required, reject requests missing an auth token or with an invalid token.
- Allow client applications to provide auth tokens for callers, attached to all gRPC requests as headers using the auth middleware. Refresh the token on unauthorized responses.

#### 4.4: Rate Limiting

- Add support for rate-limiting using an in-memory token bucket rate limiter. Create a mapping of rate limit costs to request types, such that each request (or mutation of a bidi stream) consumes a certain number of tokens. If authentication is enabled, user identifier for rate limiting is the `sub` claim from the JWT. If auth is disabled, use the client IP. Reject requests that exceed rate limits.

#### 4.5: Metrics And Telemetry

- Full OpenTelemetry and Prometheus metrics for the backend. Reuse metric names, labels, and conventions from `xmtpd` and `xmtp-node-go` where applicable.

#### 4.6: Benchmarks And Performance

- Backend benchmarks for all core database operations, including runs against a database preloaded with 100k, 1M, and 10M messages. Ensure all supported queries use database indexes and return quickly. Ensure benchmarks are run with a good diversity of queries (small topic list, large topic list, high cursors, low cursors). Prepare a detailed benchmark report.
- Attempt to optimize the schema for both read and write performance: how to unlock parallel writes without breaking total ordering per topic, how to reduce index size for common queries, how to use indexes more efficiently for the most important queries, and whether safe, low-maintenance partitioning can serve the common pattern (most queries read relatively new messages). Test changes against benchmarks.

#### 4.7: Push Subscriptions

- Port key functionality from xmtp/example-notification-server-go into the backend, allowing for clients to register push subscriptions
- Update Rust SDK to have native support for registering push subscriptions

#### 4.8: Self Publishing SDK Versions

- If someone forks this repo, how do they release their own version of the Node, Android, iOS, or Browser SDKs in a way that they can use in their own app.
- Will need to test this end-to-end across all SDKs
- Update documentation with guides for how to do this across all languages

#### 4.9: Integration Test Suite

- Create a more aggressive fork/integration test suite that can catch real-world issues and runs in CI
- Adapted from the old fork suite

### Phase 5: Extras

#### 5.1: Overhaul Message Fetching

- Complete overhaul of the streaming code to take advantage of the new backend streaming APIs.
- Add durable-progress feedback to the lease ledger. Today, a reopen after wire failure or resume replays from each lease's floor. Replay cost grows with the history since the lease opened.
- Carry a drop reason across the lease boundary. Today, transport backpressure can end a lease and fire `on_close`. P3-STR-015 intends that callback only for an explicit close or a non-retryable failure.
- <https://plan.ref.tools/W6p6z0HV0nVruZwI>

#### 5.2: Codegen SDKs

- Hollow out our iOS/Android/Browser/Node SDKs to be primarily codegen driven via Uniffi. Goal is a 90% reduction in lines of code.
- Goal here is to dramatically simplify making SDK changes, and to make the release process much much easier.

### Deferred test and tool fixes

- The keepalive probe always starts TLS in non-subscription mode. That mode cannot connect to a plaintext backend. The subscription mode honors the URL scheme.
- Investigate the iOS `testCanStreamAndUpdateNameWithoutForkingGroup` flake when tests run with `--parallel`.
- Pin the iOS health-probe package to the flake input. The same-repository pull-request gate is already in place.
