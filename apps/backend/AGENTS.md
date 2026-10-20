# xmtp-backend

Self-hosted gRPC and gRPC-Web service. Requires PostgreSQL 17 or later for durable state.

## Commands

```bash
dev/nix-shell 'SQLX_OFFLINE=true dev/agent-run cargo build --locked -p xmtp_backend'
dev/nix-shell 'just check crate xmtp_backend'
dev/nix-shell 'just backend build'
dev/nix-shell 'just backend db-up'                     # primary and replica; no image build
dev/nix-shell 'just backend s3-up'                     # local S3 target and bucket setup; no image build
dev/nix-shell 'just backend sql-prepare'               # apply migrations and update .sqlx
dev/nix-shell 'just backend sql-check'                 # verify committed .sqlx
dev/nix-shell 'just backend schema'                    # regenerate public config schema (builds xmtp_backend_config only)
dev/nix-shell 'just backend test'
dev/nix-shell 'just backend bench apps/backend/benchmarks/baseline.json' # dedicated DATABASE_URL; manual release-mode suite
dev/nix-shell 'just backend test --lib published'      # one module
dev/nix-shell 'just backend test --lib https_passthrough' # HTTPS streaming ingress check
nix build --no-link .#backend-tests.<system>          # tests and SQL check, disposable services, no Docker
dev/nix-shell 'just backend image'                     # host architecture image
dev/nix-shell 'just backend image aarch64'            # explicit architecture image
dev/nix-shell 'just backend observe-check'             # client operations, shared trace, metrics, Grafana
dev/nix-shell 'just backend tls-check'                 # build, start, check, and remove the TLS stack
dev/nix-shell 'just backend down'                      # stop the shared stack
dev/nix-shell 'just backend release'                   # stop the stack, free the slot, remove the image tag
dev/nix-shell 'just backend logs backend tempo'
dev/nix-shell 'just backend run'
```

`up` and `tls-check` load the `image` build as `XMTP_BACKEND_IMAGE`, which is
`xmtp-backend:<compose project>`, through `dev/docker/load-backend-image`. Both
Compose files run that tag. Every build has the tag
`ghcr.io/xmtp/backend:self-hosted`, so a plain `docker load` would let one
worktree's stack run another worktree's build. Do not use it for the stack.
Any other `XMTP_BACKEND_IMAGE` value is an image to run instead: start it with
`dev/docker/up`. The recipes still build, but never load a build under that tag
or remove it. `release` removes the derived tag.

`dev/nix-shell 'just backend run'` uses `dev/backend/local.toml` without attachment storage.
The `build` and `image` recipes remove `LD_LIBRARY_PATH` from the Nix process.
This lets Git fetch dependencies without the devshell's incompatible glibc path.
Compose uses `dev/backend/local-s3.toml` to offer attachment storage through
VersityGW. Both files set default and maximum query row limits to 50, so SDK
tests exercise paging. Run `dev/nix-shell 'just backend s3-up'` before attachment integration
tests. `dev/nix-shell 'just backend status'` prints the worktree's S3 URL.

`down` stops the entire shared stack, including after `db-up`.
For an explicit database-port override, set `XMTP_BACKEND_DB_PORT` and
`XMTP_BACKEND_REPLICA_PORT` before startup. Set `DATABASE_URL` to its primary
port for tests and SQL checks. Replica tests use `XMTP_BACKEND_REPLICA_PORT`.

`dev/nix-shell 'just backend tls-check'` checks the local HTTPS ingress. See
`apps/docs/src/content/docs/deploy/overview.md` for the public ingress requirements.

Set `XMTP_DATABASE_URL` and `XMTP_REPLICA_URL` for startup with the local config.
Test and SQL recipes use this worktree's database unless `DATABASE_URL` is set.
Use `dev/nix-shell 'just backend test --lib service::publish::tests'` for one module, or append
a function-name filter. Tests live beside their owning modules; shared fixtures
live in `src/test_support.rs`. The recipe defaults to four test threads to bound
local database connections; `RUST_TEST_THREADS` overrides that value.
Never silently skip database tests when the database is unavailable.
Export fixtures for other crates through `xmtp_backend/test-utils`; backend
unit tests use the existing dev-dependencies.
Use the shared `TestDatabase` guard for disposable databases. Its cleanup survives
assertion failures and test-runtime teardown; do not add success-only cleanup.
The manual database benchmarks and partitioning proposal are in `benchmarks/`.
Benchmark trials copy a disconnected fixture with a separate 120 s copy budget;
ordinary creation and cleanup retain their 5 s deadlines.
Before connecting a new test database on a streaming replica, call
`TestDatabase::wait_for_replica`; `TestServer` does this automatically. The helper
waits up to 30 seconds for the primary WAL position through the existing admin
database. Backend startup handles migration visibility after that connection.
Replica tests that pause replay use `test_support::replica::with_paused_replay`
under the replay mutex. Run this suite with `cargo test`; that mutex is process-local.
Use a total deadline for stream termination, not a new deadline for each frame.

The configuration file's types, defaults, and checks live in
`crates/xmtp_backend_config`, re-exported here as `xmtp_backend::config`; its
tests run with `dev/nix-shell 'just test crate xmtp_backend_config'`. The
backend keeps what turns configuration into behaviour: the published
`GetConfiguration` response and its size check (`src/published.rs`), inline
signing-key parsing (`src/auth/keys.rs`), and the clock checks in
`server::initialize`.

Add a new versioned file in `apps/backend/migrations/` for each schema change.
Do not edit existing migrations. Startup never deletes a database.
Keep SQLx and sqlx-cli versions aligned. SQLx query macros use the committed
`apps/backend/.sqlx` cache for builds without a database. Run SQLx prepare/check
from the backend directory through the root recipes; do not use `--workspace`.

Share payload validation and encoding with `xmtp_mls_validation` and
`xmtp_proto`. Do not depend on a client database.
Use descriptive behavior names in code, tests, and comments, not requirement IDs.
Database helpers use internal records and typed errors, never protobuf messages or
gRPC statuses. The API layer owns wire conversion and request normalization.
Important functions need `///` RustDoc explaining purpose, invariants, and relevant
errors or cancellation. Keep local implementation notes in `//` comments.
Use module-local `tests.rs` or `tests/`, including for real RPC and storage tests.

Short synchronous backend locks use `parking_lot::Mutex`, which has no poisoning
state. Keep guards short and never hold one across an await point.

API key and JWT auth is optional. `src/auth` owns key loading and verification; startup and
refresh run through `server::initialize` and `server::serve`. Auth tests use
`test_support::auth`, also exported with `test-utils`. Generate EC and Ed25519
keys per test. Reuse the process-local RSA fixture. Use the scripted JWKS fixture
for network failures. Use `api_key(name)` for a keys-only config and its value.
Use `api_key_value()` for a fresh secret value. Run `dev/nix-shell 'just backend test --lib auth::'`
for these tests.

Basic logs use `xmtp_logging`. Set `server.log_level` (default `info`) or override
with `--log-level`. `server.request_logger` defaults to true and logs completion,
including stream termination. Health Check, Watch, and List do not emit request
logs, even when the request logger is enabled. Never log payloads, topic values,
or auth headers.

`server.log_format` selects `text` (default) or `json`. `[telemetry]` sets the
metrics listener, OTLP endpoint, log export, service name, sample ratio, and
resource attributes. `OTEL_EXPORTER_OTLP_ENDPOINT` is the endpoint fallback.
See [observability](../../docs/backend-observability.md).
`src/telemetry.rs` owns `CATALOGUE`. Keep its types and help text equal to the
architecture spec and observability guide; the catalogue test checks both.
Backend metrics do not depend on trace sampling. Client span metrics do.

See `dev/fly/README.md` for development image publication and deployment.

`dev/nix-shell 'just backend ci COMMAND'` runs one command with fresh test
services from `nix/services` (`nix run .#services-test`): PostgreSQL (no
replica: the backend's replica URL is the primary, as in trunk CI), VersityGW,
anvil, the backend with a copy of `dev/backend/local-s3.toml`, and toxiproxy.
They listen on this worktree's
ports from `dev/docker/.env`; an occupied port is an error. The command exits
with the status of COMMAND and removes the service data. In CI the service logs
go to `$RUNNER_TEMP/backend-ci-logs`. `dev/services up` starts the same
services with a streaming replica in the background without Docker; see
`nix/AGENTS.md`.
