# Self-hosted backend operations

The backend serves native gRPC, gRPC-Web, and standard gRPC health on one port.
It stores durable state in PostgreSQL 18. The local stack also serves SDK tests.

## Local development

Run commands from the repository root. Use the Nix shell and the project recipes.

```sh
just backend db-up
just backend sql-prepare
just backend test
```

In the main checkout the database listens on `127.0.0.1:55432`. Other worktrees
use other ports; see [Several worktrees](#several-worktrees). Run
`just backend status` to print the ports for the checkout you are in.
The credentials and database name are in `dev/docker/compose.yml`. These
credentials are for disposable local tests only.
Set `DATABASE_URL` to select a different test database. The test user must be able
to create and delete databases. Each service test uses a separate database.
Tests live beside the modules they exercise and share one test-support module.
The test recipe uses four test threads by default to bound database connections.

`just backend db-up` starts the primary and replica without building an image.
The replica listens on `127.0.0.1:55433` in the main checkout. `just backend run`
sets both database URLs and uses `dev/backend/local.toml`.

The shared stack in `dev/docker/compose.yml` contains `db`, `replica`, `backend`,
`anvil`, `toxiproxy`, `tempo`, `prometheus`, and `grafana`.
Run `just backend up` to build and load the backend image and start all services.
Use `just backend up [services...]` to select services and their dependencies.
Use `just backend logs [services...]` for logs. Both `just backend down` and
`just backend db-down` stop the entire stack and delete its temporary state.

For migration from the old database project, run
`docker compose -p xmtp-backend down` once. If Compose cannot find the old file,
run `docker compose -f dev/docker/compose.yml -p xmtp-backend down --remove-orphans`.
The startup script prints a hint if that project still holds port 55432.

## Several worktrees

Several worktrees can run the stack at the same time. Each gets its own Compose
project and its own host ports, so no two share a database or a backend.

`dev/worktree-env` resolves the checkout to a slot and writes `dev/docker/.env`.
Compose reads that file; the recipes and `dev/docker/*` scripts source it. Ports
follow `base + slot * 100`. The main checkout is always slot 0, so its ports
never move, and a plain clone — which is what CI uses — is also slot 0.

```sh
just backend status     # project, slot, ports, and URLs for this checkout
just backend release    # stop this worktree's stack and free its slot
```

`just backend up` refuses to start when another Compose project holds one of this
worktree's ports, and names the port and the project.

Every port in this document is the slot-0 value. Do not assume it applies to a
worktree; run `just backend status`. Code reads the address from the environment:
`XMTP_BACKEND_URL` and `DATABASE_URL` in scripts and SDK tests,
`xmtp_configuration::backend_test_url()` in Rust.

Override the derivation with `XMTP_WORKTREE_NAME` or `XMTP_WORKTREE_SLOT`. Slot
claims live in `$GIT_COMMON_DIR/xmtp-worktree-slots`; entries for deleted
worktrees are pruned on the next run, so a freed slot is reused automatically.

## Local observability

Run `just backend observe-check` after `just backend up`. It checks real client
operations, both services in one trace, backend metrics, and the dashboard.
See [backend observability](../backend-observability.md) for configuration,
the metric catalogue, span names, failure modes, alerts, and client walkthroughs.

- Grafana: <http://127.0.0.1:3001>, dashboard **XMTP Backend**. Local anonymous users have Admin access.
  Port 3001, not 3000: 3000 shares a residue with Tempo's 3200 and would collide across worktree slots.
- Prometheus: <http://127.0.0.1:9090>. It scrapes the backend and Tempo every five seconds and evaluates 25 alert rules.
- Backend metrics: <http://127.0.0.1:9464/metrics>.
- Tempo: <http://127.0.0.1:3200>. OTLP receivers listen on 4317 (gRPC) and 4318
  (HTTP) inside the stack. The HTTP receiver is published on host port 4328, kept
  off 4317 so Docker does not collapse the two into a published range.

Tempo generates service graphs and span metrics and sends them to Prometheus
with exemplars. Client panels use sampled traces. Prometheus and Tempo are
provisioned as Grafana datasources. All service data is temporary.
The backend exports traces to `http://tempo:4317` inside the stack.

To check observability without a backend image, run:

```sh
./dev/docker/up db replica tempo prometheus grafana anvil
./dev/docker/up --no-deps toxiproxy
```

Toxiproxy normally depends on backend health. The second command skips that
startup dependency; proxy traffic still needs a running backend.
Tempo has no container healthcheck because its image is distroless.
Check its readiness with `curl -sf http://127.0.0.1:3200/ready`.

## Configuration

Use `--config` to select one TOML file. The primary database URL is required.
Other settings have defaults. Unknown keys and invalid values fail startup.
The config schema is in `docs/schemas/backend-v1.json`. After changing the typed
config, run `just backend schema` to regenerate it, then run the config tests.

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
The local stack configures the OTLP endpoint for Tempo.

`server.log_format` selects `text` (default) or `json`. `[telemetry]` configures
the metrics listener, OTLP endpoint, optional log export, service name, sample
ratio, and resource attributes. An absent endpoint uses
`OTEL_EXPORTER_OTLP_ENDPOINT`. Resource attributes are exported verbatim; never
put secrets in them. Backend metrics do not depend on trace sampling.
Tempo-derived client metrics depend on `sample_ratio` and successful export.

## Schema changes and builds

Until completion of Phase 6, edit the single backend migration. There are no
permanent deployments that need an incremental upgrade path. Recreate the
disposable database after a migration edit, then refresh checked SQL metadata:

```sh
just backend db-down
just backend db-up
just backend sql-prepare
just backend sql-check
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
just backend build
just backend image
dev/nix-shell 'nix build .#backend-image-aarch64-unknown-linux-musl'
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
Run this check with `just backend test --lib https_passthrough`.

Shutdown stops request admission and ends active subscriptions. Unary requests
already admitted can finish within `server.max_drain_duration_ms`. At the end of
that budget, the service cancels remaining handlers and connection IO. Clients
must reconnect to another instance with their safe topic cursors. A dropped
response does not establish whether a publish committed.
Shutdown marks both aggregate health and every named RPC service `NOT_SERVING`.

Caller authentication and caller quotas are not implemented until Phase 6.
Do not expose this unauthenticated service to untrusted traffic.

Expiry is metadata until Phase 5. The service does not prune rows or hide them
at read time. A failed publish response does not prove rollback. Retry the exact
canonical envelope bytes to recover the original metadata while the rows exist.
