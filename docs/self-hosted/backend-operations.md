# Self-hosted backend operations

The backend serves native gRPC, gRPC-Web, and standard gRPC health on one port.
It stores durable state in PostgreSQL 18. Client SDK integration is separate work.

## Local development

Run commands from the repository root. Use the Nix shell and the project recipes.

```sh
just backend-db-up
just backend-sql-prepare
just test-backend
```

The database listens on `127.0.0.1:55432`. The credentials and database name are
in `dev/backend/compose.yml`. These credentials are for disposable local tests only.
Set `DATABASE_URL` to select a different test database. The test user must be able
to create and delete databases. Each service test uses a separate database.
Tests live beside the modules they exercise and share one test-support module.
The test recipe uses four test threads by default to bound database connections.

Set `XMTP_DATABASE_URL` before starting the service with the example config:

```sh
dev/nix-shell 'cargo run --locked -p xmtp_backend -- --config dev/backend/config.toml'
```

The default listener is `0.0.0.0:5050`. Use another listener address when the old
SDK test services already use that port. Do not replace those services before
the SDK integration phase.

## Configuration

Use `--config` to select one TOML file. The primary database URL is required.
Other settings have defaults. Unknown keys and invalid values fail startup.
The config schema is in `docs/schemas/backend-v1.json`. After changing the typed
config, run `just backend-schema` to regenerate it, then run the config tests.

A value such as `env:XMTP_DATABASE_URL` reads an environment variable at startup.
Keep secrets in environment variables. Do not commit them in config files.
Configure each chain RPC route needed for smart-contract-wallet signatures.
An empty chain map supports identities that do not need chain RPC verification.

The optional replica URL must name one physical PostgreSQL replica, not a load
balancer that selects independently lagging replicas. Publish and Query use the
primary. Newest, Get, identifier lookup, and subscriptions use the selected read
database. Those reads can lag behind a successful publish.

The tailer keeps one dedicated connection to the selected read database. This is
in addition to the configured request pools. Without a replica, budget at most
`max_connections + 1` backend connections per instance. With a replica, budget
`max_connections` on the primary and `max_connections + 1` on the replica.
The boundary worker uses the primary pool. Keeping the tailer separate permits
a one-connection request pool and makes a lost tailer connection observable.

## Logging

Set `server.log_level` to `off`, `error`, `warn`, `info`, `debug`, or `trace`.
The default is `info`. The `--log-level` CLI flag overrides the config value.
The service uses `xmtp_logging`; no separate subscriber or OTEL setup is needed.

`server.request_logger` defaults to `true`. At INFO, it emits one completion event
with `method`, `duration_ms`, `request_size_bytes`, `response_size_bytes`, and a generated `request_id`.
Request size counts HTTP body bytes consumed, including gRPC framing and any
compression. Bidirectional streams accumulate this count until the response ends
or is cancelled. Headers are not counted, and request bodies are not buffered.
Response size counts emitted body bytes, including gRPC-Web body framing, without
buffering the response. It does not confirm client receipt. On cancellation or
failure, it records only bytes emitted before completion. HTTP trailers are not
counted; gRPC-Web trailers encoded as body data are counted.

Accepted stream mutations log added and removed topic counts at INFO. They do
not log topic values or payloads. Turning off the request logger suppresses only
completion events; use a higher log level to suppress INFO mutation events too.
Full OpenTelemetry configuration is deferred to Phase 4.

## Schema changes and builds

Until completion of Phase 6, edit the single backend migration. There are no
permanent deployments that need an incremental upgrade path. Recreate the
disposable database after a migration edit, then refresh checked SQL metadata:

```sh
just backend-db-down
just backend-db-up
just backend-sql-prepare
just backend-sql-check
```

Stopping this test topology deletes its temporary database state. It cannot be
recovered. Service startup never deletes a database when a migration has changed.
At completion of Phase 6, freeze the migration and use new migrations for later
schema changes.

Commit `apps/backend/.sqlx` metadata with the SQL change. The root recipes run
prepare/check from that package, without `--workspace`. SQLx and its CLI must use matching
versions. The backend uses SQLx 0.9 because 0.8 conflicts with the workspace's
SQLite bindings during Cargo dependency resolution. Postgres remains a
backend-only dependency.

```sh
dev/nix-shell 'SQLX_OFFLINE=true cargo build --locked -p xmtp_backend'
just build-backend
just backend-image
just backend-image aarch64
```

Nix builds need no database. The images use `ghcr.io/xmtp/backend:self-hosted`
and accept `--config` with a mounted config file.

## Transport and retention

Tonic handles gRPC-Web directly. A public deployment must terminate HTTPS at a
trusted load balancer and pass requests to the backend without protocol
conversion. Preserve CORS preflight, authorization and version headers, gRPC
trailers, and incremental response frames. Do not buffer streaming responses.
The plaintext backend listener is not a public endpoint.

The backend test suite includes an HTTPS ingress check. A test-only TLS terminator
passes HTTP bytes to the service without gRPC conversion. The check requires a
Started frame before publication, then requires the published envelope while the
same response stays open. It also checks CORS, request headers, and error details.
Run this check with `just test-backend --lib https_passthrough`.

Shutdown stops request admission and ends active subscriptions. Unary requests
already admitted can finish within `server.max_drain_duration_ms`. At the end of
that budget, the service cancels remaining handlers and connection IO. Clients
must reconnect to another instance with their safe topic cursors. A dropped
response does not establish whether a publish committed.

Caller authentication and caller quotas are not implemented until Phase 6.
Do not expose this unauthenticated service to untrusted traffic.

Expiry is metadata until Phase 5. The service does not prune rows or hide them
at read time. A failed publish response does not prove rollback. Retry the exact
canonical envelope bytes to recover the original metadata while the rows exist.
