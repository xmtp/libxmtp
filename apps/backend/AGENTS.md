# xmtp-backend

Self-hosted gRPC and gRPC-Web service. Requires PostgreSQL 17 or later for durable state.

## Commands

```bash
dev/nix-shell 'SQLX_OFFLINE=true cargo build --locked -p xmtp_backend'
just check crate xmtp_backend
just backend build
just backend db-up                     # primary and replica; no image build
just backend sql-prepare               # apply migration and update .sqlx
just backend sql-check                 # verify committed .sqlx
just backend schema                    # regenerate public config schema
just backend test
just backend test --lib config         # one module
just backend test --lib https_passthrough # HTTPS streaming ingress check
just backend image                     # host architecture image
just backend image aarch64            # explicit architecture image
just backend up                        # SDK and observability services
just backend observe-check             # client operations, shared trace, metrics, Grafana
just backend tls-check                 # build, start, check, and remove the TLS stack
just backend down                      # stop the shared stack
just backend logs backend tempo
just lint-rust
just backend run
just backend db-down
```

`just backend run` uses `dev/backend/local.toml`. Its default and maximum query
row limits are both 50, so SDK tests exercise paging.

The shared stack is in `dev/docker/compose.yml`. Database ports are 55432
(primary) and 55433 (replica). `db-down` stops the entire shared stack.
For a separate local stack, set `XMTP_BACKEND_DB_PORT` and
`XMTP_BACKEND_REPLICA_PORT` before startup. Set `DATABASE_URL` to its primary
port for tests and SQL checks. Replica tests use `XMTP_BACKEND_REPLICA_PORT`.

`dev/tls/` contains the reusable HAProxy config and the separate `libxmtp-tls`
validation stack. Run `dev/nix-shell 'just backend tls-check'` for native gRPC
trailers, HTTP/1.1 gRPC-Web, CORS, incremental streaming, and TLS health checks.
It uses port 18443, an internal PostgreSQL 18 database, and a disposable
self-signed certificate. The combined certificate/key PEM is readable by the
non-root HAProxy user. Do not use this certificate for deployment.
The check needs `grpcurl`, `protoc`, Python 3, OpenSSL, Docker, and Nix.
It gets `grpc-health-probe` from `nix shell nixpkgs#grpc-health-probe`.

For the deliberate idle-stream failure check, run these through `dev/nix-shell`:
`dev/tls/up`, `dev/tls/check.sh --short-timeout`, then `dev/tls/down`.
The check uses 12-second client/server timeouts plus a one-hour tunnel timeout.
It requires an acknowledged HTTP/2 PING before the stream drops, then restores
the normal config on exit.
HTTP/2 uses client/server timeouts, not `timeout tunnel`. Connection-level
PING frames do not refresh stream timers. Application keepalive messages do
carry stream data, but the backend's default 30-second interval cannot prevent
a 12-second timeout. The normal config uses 24-hour timeouts.

This local stack cannot prove Railway TCP proxy passthrough, the assigned
public port, or `*.railway.internal` DNS behavior. Check those on Railway.
For a config parse check, run `dev/tls/up --cert-only`, set `XMTP_TLS_PEM` to
the absolute `dev/tls/.generated/server.pem` path, and set `XMTP_TLS_BACKEND`
to `backend:5050`. Then run
`nix shell nixpkgs#haproxy --command haproxy -c -f dev/tls/haproxy.cfg`.

Set `XMTP_DATABASE_URL` and `XMTP_REPLICA_URL` for startup with the local config. Test and SQL recipes default to
`postgres://xmtp:xmtp@localhost:55432/xmtp_backend` in the main checkout, and a
shifted port in any other worktree; `DATABASE_URL` overrides it.
Use `just backend test --lib service::publish::tests` for one module, or append
a function-name filter. Tests live beside their owning modules; shared fixtures
live in `src/test_support.rs`. The recipe defaults to four test threads to bound
local database connections; `RUST_TEST_THREADS` overrides that value.
Never silently skip database tests when the database is unavailable.
Use the shared `TestDatabase` guard for disposable databases. Its cleanup survives
assertion failures and test-runtime teardown; do not add success-only cleanup.
Replica tests that pause replay use `test_support::replica::with_paused_replay`
under the replay mutex. Run this suite with `cargo test`; that mutex is process-local.
Use a total deadline for stream termination, not a new deadline for each frame.

Keep one mutable migration through completion of Phase 6. Recreate only the
disposable backend database after schema edits. Startup never deletes a database.
Keep SQLx and sqlx-cli versions aligned. SQLx query macros use the committed
`apps/backend/.sqlx` cache for builds without a database. Run SQLx prepare/check
from the backend directory through the root recipes; do not use `--workspace`.

Read the approved specs and style guide. Share payload validation and encoding
with `xmtp_mls_validation` and `xmtp_proto`. Do not depend on a client database.
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
Use `api_key_value()` for a fresh secret value. Run `just backend test --lib auth::`
for these tests.

Basic logs use `xmtp_logging`. Set `server.log_level` (default `info`) or override
with `--log-level`. `server.request_logger` defaults to true and logs completion,
including stream termination. Never log payloads, topic values, or auth headers.

`server.log_format` selects `text` (default) or `json`. `[telemetry]` sets the
metrics listener, OTLP endpoint, log export, service name, sample ratio, and
resource attributes. `OTEL_EXPORTER_OTLP_ENDPOINT` is the endpoint fallback.
The stack includes `db`, `replica`, `backend`, `anvil`, `toxiproxy`, `tempo`,
`prometheus`, and `grafana`. See [observability](../../docs/backend-observability.md).
`src/telemetry.rs` owns `CATALOGUE`. Keep its types and help text equal to the
architecture spec and observability guide; the catalogue test checks both.
Backend metrics do not depend on trace sampling. Client span metrics do.

Nix outputs: `xmtp-backend`, `backend-image`,
`backend-image-x86_64-unknown-linux-musl`, and
`backend-image-aarch64-unknown-linux-musl`. `just backend image [target_arch]`
builds the host target by default or a named musl architecture. Both images use the
`xmtp-backend` entry point and the `ghcr.io/xmtp/backend:self-hosted` tag.

## Ephemeral test backends

`just test` needs `just backend up db replica` and the shared backend services.
It sets `SQLX_OFFLINE=true` for compilation and `DATABASE_URL` for test runs.
The database URL defaults to this worktree's database; `just backend status`
prints it.
Native `xmtp_mls` tests can use `EphemeralBackend::start(toml)` and
`tester!(alix, backend: &backend)` with optional `auth: callback`.
This helper is available only under `cfg(test)`, not `xmtp_mls/test-utils`.
The backend exports its fixtures through `xmtp_backend/test-utils`. Its own
`cargo test` still uses the existing dev-dependencies.

Use the shared backend on port 5050 by default. Use an ephemeral backend only
when a test needs a specific configuration. Nextest runs each test in its own
process, so tests cannot share an ephemeral backend. Each such test pays for
a database create, a migration, and a listener bind.
