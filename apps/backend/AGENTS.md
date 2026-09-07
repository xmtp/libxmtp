# xmtp-backend

Self-hosted gRPC and gRPC-Web service. PostgreSQL 18 stores durable state.

## Commands

```bash
dev/nix-shell 'SQLX_OFFLINE=true cargo build --locked -p xmtp_backend'
just check crate xmtp_backend
just build-backend
just backend-db-up
just backend-sql-prepare               # apply migration and update .sqlx
just backend-sql-check                 # verify committed .sqlx
just backend-schema                    # regenerate public config schema
just test-backend
just test-backend --lib config         # one module
just test-backend --lib https_passthrough # HTTPS streaming ingress check
just backend-image                     # amd64 image
just backend-image aarch64             # arm64 image
just lint-rust
dev/nix-shell 'cargo run --locked -p xmtp_backend -- --config dev/backend/config.toml'
just backend-db-down
```

Set `XMTP_DATABASE_URL` for service startup. Test and SQL recipes default to
`postgres://xmtp:xmtp@localhost:55432/xmtp_backend`; `DATABASE_URL` overrides it.
Use `just test-backend --lib service::publish::tests` for one module, or append
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

Basic logs use `xmtp_logging`. Set `server.log_level` (default `info`) or override
with `--log-level`. `server.request_logger` defaults to true and logs completion,
including stream termination. Never log payloads, topic values, or auth headers.

Nix outputs: `xmtp-backend`, `backend-image`, and
`backend-image-aarch64-unknown-linux-musl`. Both images use the `xmtp-backend`
entry point and the `ghcr.io/xmtp/backend:self-hosted` tag.
