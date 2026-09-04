# XMTP Self-Hosted Transition

XMTP is replacing both the v3 network (`xmtp-node-go`) and the v4 network (`xmtpd`) with one self-hosted backend. The backend does not exist today. This document is the plan of record for the project. Future agents must treat it, and the approved specs in `docs/specs`, as authoritative.

## Compatibility

- There is no expectation of live message migration, or of perfect compatibility at the wire level, with either existing version.
- Compatibility is expected at the client SDK level. With minimal setup changes (a backend URL instead of `env`, and new client authentication) an application built on the existing SDKs should be able to connect to a self-hosted backend without changing application code. The goal is to minimize changes that touch application code.
- Those clients are expected to start with a clean local database the first time they connect to a self-hosted backend.

## Outcome

- A new backend, `apps/backend`, written in Rust. It takes the best parts of `xmtpd` and `xmtp-node-go` and implements the bare minimum API surface an XMTP client needs to exercise the core SDK functionality: registering identities, publishing envelopes, querying envelopes, and efficiently streaming envelopes. It should be a single binary that can be horizontally scaled and load balanced, with no local state.
- `libxmtp` and the platform SDKs are overhauled to work with this backend exclusively. Dead code for decentralization, blockchains, the payer service, and originator IDs is removed. The complexity `xmtpd` and v4 added for ordering messages between originators is removed. Streaming is expected to be simplified.
- The `xmtpd`, `xmtp-node-go`, and `proto` repositories are deprecated. The entire stack lives in `libxmtp`. All `.proto` files live in a `proto/` folder in this repository.
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

Expected specs by the end of the project:

| Spec | Content |
| --- | --- |
| `001_backend_api.md` | Public API of the backend |
| `002_backend_architecture.md` | Backend service design and database schema |
| `003_message_security.md` | Adaptation of the `xmtp_mls` README |
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

Ephemeral documents produced in Phase 0 live in `docs/self-hosted`: the existing-behavior wiki in `existing/`, the deletion inventory in `deletions.md`, the test catalog in `tests/`, and the list of deleted tests in `tests-to-delete.md`. They are deleted when the project ends.

## Phases

### Phase 0: Mise en place

- Dispatch sub-agents to research the existing implementations in `libxmtp`, `xmtp-node-go`, `xmtpd`, and `proto`, and catalog all current behaviors and requirements of the existing endpoints in a detailed wiki at `docs/self-hosted/existing`. Required content: the input parameters of each endpoint and their exact formats (serialization, bindings of fields to database tables), the database schema, what conditions trigger errors and how errors are surfaced to the client, limits applied to endpoints, rate limiting, and anything else relevant to future implementers. All claims cite function names and file paths. The goal is a complete and accurate specification of the relevant parts of the existing services.
- Interrogate the proposed `docs/self-hosted/backend.proto`. Will it lead to a performant backend that can handle all needs of the new client? Analyze the expected callers of each backend API in `libxmtp` and ensure their core business requirements can be met.
- Look for macros, utilities, and coding practices that are idiomatic in this repository. Create `docs/self-hosted/style-guide.md`.
- Review `libxmtp` and determine what code can be removed by the end of the project: a concrete list of deletions, and the downstream change of each.
- Refine this document: shorter, tighter, internally consistent.
- Audit all `AGENTS.md` and `CLAUDE.md` files. They must be up to date with the code at project start and extremely concise. Prefer `AGENTS.md` over `CLAUDE.md`; each `AGENTS.md` has a sibling `CLAUDE.md` pointer. Each package, crate, and app involved in this project has an `AGENTS.md` with the basic commands to build, check, and test the package, and to test a single file or function. Language can be borderline caveman.
- Write `docs/self-hosted/guidelines.md`.
- Using the test report in `docs/self-hosted/tests/`, take a first pass at tests that will not be needed at the end of the project, and create a pull request that deletes them now. The same pull request deletes `apps/xnet` and its Nix references.

Expected pull requests: a stack of two, one for documentation changes and one for test deletions. They can be worked on in parallel worktrees.

### Phase 1: Scaffolding

- Scaffold `apps/backend`. Ensure it builds with Nix. Give it a hello-world main and a single test.
- If new crates are needed for shared types, utilities, and structs required by both the backend and `xmtp_mls`, scaffold them too. Ensure they build and test in CI.
- Audit all of `crates/xmtp_mls`, including its runtime and test utilities, against the expected scope of the backend API. Move every function, struct, utility, and type the backend will share out of `xmtp_mls` and into the appropriate other crate. Spec 002 must be written before this audit; without it the comparison cannot be accurate.
- Ensure the backend can produce a Docker image, the way the MLS validation service is built with Nix. Ensure all check, build, and test commands work and maximize Nix caching.
- Create the `proto/` folder for all `.proto` files. Copy every required file from the `proto` repository (including files for endpoints this project removes, such as v4) plus the new backend protos. Set up Buf linting in the justfile. Update all scripts and `crates/xmtp_proto` to make this folder authoritative, and delete the old generated tree and the `proto` repository dependency in the same phase.
- Ensure tests for the new crates run in CI.
- Temporarily disable these GitHub Actions to speed up CI: anything for xdbg, wasm, the browser SDK, and `nightly-protos.yml`. The browser SDK is handled in a later phase.

### Phase 2: Backend

Specs 001 and 002 must be completed and approved before this phase begins. This phase sets up the backend, creates its tests, and implements the complete API surface. The backend is not integrated into any client or SDK yet, except for minimal stateless test harnesses required by the test suite.

- XIP-83 style bidirectional streaming (<https://github.com/xmtp/XIPs/pull/139>) as well as traditional HTTP streaming.
- Complete support for the API surface defined in `backend.proto`. The standard gRPC health service is served. There is no version or metadata endpoint in v1.
- A Postgres schema designed for the API surface, with indexes for every query parameter.
- A single binary that can be horizontally scaled and load balanced. The MLS validation service is not used; the same lookups happen in the shared crate, which receives the validation logic in this phase.
- No rate limits, authentication, or authorization. Later phases add them.
- Establish, and include in the spec, a concise TOML config format for all server configuration. Config files may reference environment variables for secrets. The format should have a defined schema that can be publicly hosted and referenced by config files that support Taplo schemas.

### Phase 3: Integration

Replace all backend selection in `xmtp_mls` with the self-hosted backend. This requires updates to every binding in `bindings/`, every SDK in `sdks/`, and the CLIs in `apps/`. The diff is large and changes the test harness of every client SDK. `docs/self-hosted/deletions.md` gives the order of the deletions in this phase.

- XIP-83 bidirectional streaming becomes the only native stream path. The opt-in flag is removed and the legacy streaming stack is deleted.
- `apps/xmtp_debug` stays as an app. Its backend selection and other dead functionality are deleted as the code they depend on goes.
- The SCW verifier tests start a local `anvil` from Rust instead of the Docker service.

We should be able to remove the `node`, `node-web`, `validation`, `anvil`, and `mlsdb` services from `dev/docker/docker-compose.yml` and have CI pass. A new `backend` service, built from the Phase 2 backend, serves the SDK tests and reuses the existing `db` service.

### Phase 4: Metrics, benchmarks, performance

- Full OpenTelemetry and Prometheus metrics for the backend. Reuse metric names, labels, and conventions from `xmtpd` and `xmtp-node-go` where applicable.
- Backend benchmarks for all core database operations, including runs against a database preloaded with 1M messages. Ensure all supported queries use database indexes.
- Attempt to optimize the schema for both read and write performance: how to unlock parallel writes without breaking total ordering per topic, how to reduce index size for common queries, how to use indexes more efficiently for the most important queries, and whether safe, low-maintenance partitioning can serve the common pattern (most queries read relatively new messages).

### Phase 5: Message pruning

- Figure out how to expire messages from the backend database.
- Every row carries an expiry set at publish time from its topic kind. Group messages and key packages expire after 3 months. Identity updates never expire. The periods for welcome messages and commit-log entries are still to be decided.
- Define what a client does with a cursor that points below expired rows before retention is enabled in production.

### Phase 6: Authentication and rate limiting

- Allow client applications to provide auth tokens for callers, attached to all gRPC requests as headers.
- Take a similar rate limiting approach.
