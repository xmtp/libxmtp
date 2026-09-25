# Self-hosted backend operations

The backend serves native gRPC, gRPC-Web, and standard gRPC health on one port.
It stores durable state in PostgreSQL 17 or later. The local stack also serves SDK tests.

## Local development

Run commands from the repository root. Use the Nix shell and the project recipes.

```sh
just backend db-up
just backend sql-prepare
just backend test
just backend s3-up
```

In the main checkout the database listens on `127.0.0.1:55432`. Other worktrees
use other ports; see [Several worktrees](#several-worktrees). Run
`just backend status` to print the ports for the checkout you are in.
The credentials and database name are in `dev/docker/compose.yml`. These
credentials are for disposable local tests only.
The VersityGW target uses the worktree port shown by `just backend status`.
`s3-init` creates the `attachments` bucket, permits public GET, and sets CORS
for signed PUT and GET requests. The S3 integration test needs both services.
Compose uses `dev/backend/local-s3.toml` to offer attachments. The
`dev/backend/local.toml` file starts without storage target settings or S3
environment variables, including on the iOS Fly test backend.
Set `DATABASE_URL` to select a different test database. The test user must be able
to create and delete databases. Each service test uses a separate database.
Tests live beside the modules they exercise and share one test-support module.
The test recipe uses four test threads by default to bound database connections.

`just backend db-up` starts the primary and replica without building an image.
The replica listens on `127.0.0.1:55433` in the main checkout. `just backend run`
sets both database URLs and uses `dev/backend/local.toml`.

The shared stack in `dev/docker/compose.yml` contains `db`, `replica`, `backend`,
`s3`, `s3-init`, `anvil`, `toxiproxy`, `tempo`, `prometheus`, and `grafana`.
Run `just backend up` to build and load the backend image and start all services.
Use `just backend up [services...]` to select services and their dependencies.
Use `just backend logs [services...]` for logs. Both `just backend down` and
`just backend db-down` stop the entire stack and delete its temporary state.
They preserve the disk-backed Tempo trace volume. `just backend release` also
deletes that volume and frees the worktree's port slot.

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
provisioned as Grafana datasources. Tempo retains trace blocks for six hours on
a worktree-scoped disk volume. The other services use temporary storage. See
[local trace storage](../backend-observability.md#local-trace-storage) for
retention timing, disk use, and volume cleanup.
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

Use `--config` or `XMTP_CONFIG` to supply inline TOML. Use `--config-file` for a
file path. Supply exactly one source. `server.identifier` and the primary
database URL are required. Other settings have defaults. Unknown keys and
invalid values fail startup.
The config schema is in `docs/schemas/backend-v1.json`. After changing the typed
config, run `just backend schema` to regenerate it, then run the config tests.

`server.identifier` names this deployment, by convention a reverse-DNS name such
as `org.xmtp.dev`. It is 1 to 256 bytes with no whitespace or control
characters. It must never change once clients have connected: every client
database is bound to the identifier it first saw, and a client that reaches a
backend publishing a different one stops with a backend-mismatch error. Hosts,
ports, and URLs may change freely.

`server.min_libxmtp_version` is an optional semantic version. It is published to
clients, which refuse to build below it. Only major, minor, and patch are
compared. The backend does not yet reject requests by version.

`[mls]` carries advisory group policy (`max_group_members`,
`max_installations_per_inbox`, `commit_log_enabled`) that the backend publishes
but does not enforce. `limits.max_request_bytes` and `limits.max_response_bytes`
are capped at the fixed 25 MiB transport ceiling.

`ConfigurationService.GetConfiguration` publishes these settings without a
credential. The response is built once at startup and never changes while the
process runs. It carries no URL, key material, leeway, refresh timing, or
database, telemetry, or listener setting, and startup fails if it encodes to more
than 64 KiB.

A value such as `env:XMTP_DATABASE_URL` reads an environment variable at startup.
Keep secrets in environment variables. Do not commit them in config files.
Configure each chain RPC route needed for smart-contract-wallet signatures.
An empty chain map supports identities that do not need chain RPC verification.

The optional replica URL must name one physical PostgreSQL replica, not a load
balancer that selects independently lagging replicas. Publish and Query use the
primary. Newest, identifier lookup, and subscriptions use the selected read
database. Those reads can lag behind a successful publish.

The tailer keeps one dedicated connection to the selected read database. This is
in addition to the configured request pools. Without a replica, budget at most
`max_connections + 1` backend connections per instance. With a replica, budget
`max_connections` on the primary and `max_connections + 1` on the replica.
The boundary worker uses the primary pool. Keeping the tailer separate permits
a one-connection request pool and makes a lost tailer connection observable.

## Push providers

Add a provider block to enable registrations for that channel. Each backend
supports one APNs key and one Firebase project. Credentials load at startup;
restart the backend after you change them. Keep private keys and service-account
JSON in your secret manager. Pass their contents through environment variables,
not file paths. The backend does not put them in logs, errors, or Debug output.

```toml
[push.apns]
key = "env:XMTP_APNS_KEY"
key_id = "ABC123DEFG"
team_id = "TEAM123456"
bundle_id = "org.example.app"
environment = "production"

[push.fcm]
service_account = "env:XMTP_FCM_SERVICE_ACCOUNT"
```

### APNs credentials

In your Apple developer account, enable push notifications for the app. Then
[create a private key](https://developer.apple.com/help/account/keys/create-a-private-key)
with the APNs capability. Download the `.p8` file and store it securely. Apple
does not let you download that key again. Set `XMTP_APNS_KEY` to its full PKCS#8
PEM contents, including the header, footer, and line breaks.

Set `key_id` to the key identifier and `team_id` to your Apple developer team
identifier. Set `bundle_id` to the app's exact bundle identifier. The app and
key must support the selected environment. Use `sandbox` for development
tokens and `production` for production tokens. A development token sent to
the production host does not deliver a push. The backend selects the fixed
Apple host for that environment and refreshes its provider JWT every 55 minutes.

### FCM credentials

Enable the Firebase Cloud Messaging API in the app's Firebase project. In
Firebase **Project settings > Service accounts**, create a service-account
private key and download the JSON file. Follow the
[FCM server authentication guide](https://firebase.google.com/docs/cloud-messaging/auth-server)
for the required account permissions. Set `XMTP_FCM_SERVICE_ACCOUNT` to the
complete JSON contents. The backend reads `project_id` from that JSON; there
is no separate project setting and no default credential discovery.

The JSON must contain a valid RSA private key, `client_email`, `project_id`,
and `token_uri` equal to `https://oauth2.googleapis.com/token`. OAuth tokens
use only the `firebase.messaging` scope. The backend caches them and permits
only one refresh at a time. It does not follow redirects during token exchange
or message delivery.

For Apple apps reached through FCM, also
[upload the APNs authentication key to Firebase](https://firebase.google.com/docs/cloud-messaging/ios/get-started).
A service account alone does not enable Apple delivery. The backend sends
data-only messages, with high priority for Android and background priority 5
for Apple. The app fetches and decrypts the message before it shows a notification.

### Delivery failures

Watch `xmtp_push_deliveries_total{channel="apns",outcome="mismatch"}` and
`xmtp_push_deliveries_total{channel="fcm",outcome="mismatch"}`. A rising count can
mean a wrong APNs environment, bundle identifier, or Firebase project. Affected
recipients keep their registrations and subscriptions. New pushes resume after
you correct the config and restart the backend; clients need not register again.
Failed past deliveries are not replayed. Normal recipient expiry still applies.

An APNs `410 Unregistered` or `410 ExpiredToken`, or an FCM `UNREGISTERED` detail,
deletes the dead recipient and its subscriptions. Other answers do not delete
them. Removing a provider block counts later deliveries for that channel as
`failed`, emits a warning, and keeps the registrations.

Each attempt has one 10 second deadline, including credentials and response
bodies. Response bodies, including OAuth responses, are limited to 16 KiB.
FCM quota retries wait at least 60 seconds. Other FCM retry responses honor
`Retry-After`, with a maximum of 300 seconds. Transient attempts stop at
`push.max_attempts`. See [backend observability](../backend-observability.md)
for the complete metric catalogue.

## Logging

Set `server.log_level` to `off`, `error`, `warn`, `info`, `debug`, or `trace`.
The default is `info`. The `--log-level` CLI flag overrides the config value.
The service uses `xmtp_logging`; no separate subscriber or OTEL setup is needed.

`server.request_logger` defaults to `true`. At INFO, it emits one completion event
with `method`, `duration_ms`, `request_size_bytes`, `response_size_bytes`, a
generated `request_id`, and the backend `identifier`.
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
`OTEL_EXPORTER_OTLP_ENDPOINT`. The identifier is exported as the
`xmtp.backend.identifier` resource attribute and is reserved like `service.name`. Resource attributes are exported verbatim; never
put secrets in them. Backend metrics do not depend on trace sampling.
Tempo-derived client metrics depend on `sample_ratio` and successful export.

## Schema changes and builds

Add a new versioned file in `apps/backend/migrations/` for each schema change.
Keep existing migrations unchanged. Apply pending migrations to the local primary,
then refresh checked SQL metadata:

```sh
just backend db-up
just backend sql-prepare
just backend sql-check
```

Test upgrades from an existing database and creation of a fresh database.
Service startup never deletes a database.

Commit `apps/backend/.sqlx` metadata with the SQL change. The root recipes run
prepare/check from that package, without `--workspace`. SQLx and its CLI must use matching
versions. The backend uses SQLx 0.9 because 0.8 conflicts with the workspace's
SQLite bindings during Cargo dependency resolution. Postgres remains a
backend-only dependency.

```sh
dev/nix-shell 'SQLX_OFFLINE=true cargo build --locked -p xmtp_backend'
just backend build
just backend image
just backend image x86_64
just backend image aarch64
dev/nix-shell 'nix build .#backend-image-aarch64-unknown-linux-musl'
```

Nix builds need no database. The images use `ghcr.io/xmtp/backend:self-hosted`
and accept inline TOML through `--config` or `XMTP_CONFIG`. Use `--config-file`
with a mounted config file. Supply exactly one source.

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

### Local TLS checks

`dev/tls/` contains the reusable HAProxy config and a separate `libxmtp-tls`
validation stack. Run `dev/nix-shell 'just backend tls-check'` to check native
gRPC trailers, HTTP/1.1 gRPC-Web, CORS, incremental streaming, and TLS health.
The stack uses port 18443, an internal PostgreSQL 18 database, and a disposable
self-signed certificate. Do not use that certificate for deployment. The check
needs `grpcurl`, `protoc`, Python 3, OpenSSL, Docker, and Nix; it gets
`grpc-health-probe` from `nix shell nixpkgs#grpc-health-probe`.

To test an idle-stream failure, run `dev/tls/up`,
`dev/tls/check.sh --short-timeout`, and `dev/tls/down` through `dev/nix-shell`.
The check uses 12-second client and server timeouts and a one-hour tunnel
timeout. It confirms an HTTP/2 PING before the stream drops, then restores the
normal 24-hour timeouts. HTTP/2 uses client and server timeouts, not `timeout
tunnel`. PING frames do not refresh stream timers. Application keepalive
messages carry stream data, but the default 30-second interval cannot prevent
a 12-second timeout.

This local stack cannot prove Railway TCP proxy passthrough, the public port,
or `*.railway.internal` DNS behavior. Check those on Railway. To check only the
HAProxy config, run `dev/tls/up --cert-only`, set `XMTP_TLS_PEM` to the absolute
`dev/tls/.generated/server.pem` path and `XMTP_TLS_BACKEND=backend:5050`, then
run `nix shell nixpkgs#haproxy --command haproxy -c -f dev/tls/haproxy.cfg`.

Shutdown stops request admission and ends active subscriptions. Unary requests
already admitted can finish within `server.max_drain_duration_ms`. At the end of
that budget, the service cancels remaining handlers and connection IO. Clients
must reconnect to another instance with their safe topic cursors. A dropped
response does not establish whether a publish committed.
Shutdown marks both aggregate health and every named RPC service `NOT_SERVING`.

Optional API key and JWT authentication is available through the `[auth]` config section.
The section must state `enabled = true` or `enabled = false`; a section without
it fails startup, so auth cannot switch off by accident. A disabled section
checks no credential and loads no key material.
See the [authentication configuration](../../apps/docs/src/content/docs/get-started/run-the-backend.mdx#auth)
to require valid bearer credentials. Caller quotas are not implemented. A valid token
does not prove group membership. Health and `ConfigurationService` are served without a credential whatever the
setting is; no other method is exempt.
Without `[auth]`, the service is unauthenticated.
Do not expose an unauthenticated service to untrusted traffic.

Expiry is metadata until Phase 5. The service does not prune rows or hide them
at read time. A failed publish response does not prove rollback. Retry the exact
canonical envelope bytes to recover the original metadata while the rows exist.
