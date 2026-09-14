---
title: Deploy on Fly.io
description: Run the published XMTP backend image with Fly Proxy and Managed Postgres.
---

Fly injects your configuration file into the published image. No image build is
needed. Read the [deployment overview](/deploy/overview/) first for image tags,
ports, connection budgets, health, shutdown, ingress, and security requirements.

**Image requirement:** `--config-file` requires an image built from the
configuration flag change in commit
`7d44488fe14ce903382a2b24c52cefe33c4f0e93` or later. Pin a published image with
`ghcr.io/xmtp/backend:sha-<commit>`. At the time of verification, `:self-hosted`
predated this change and could not start with this guide's configuration.

## Create the configuration

Install [flyctl](https://fly.io/docs/flyctl/install/) and run `fly auth login`.
Create an empty directory. Save these two files in it. Replace
`my-xmtp-backend` with a unique app name. This example uses San Jose (`sjc`).

`fly.toml`:

```toml
app = "my-xmtp-backend"
primary_region = "sjc"
kill_signal = "SIGTERM"
kill_timeout = "30s"

[build]
image = "ghcr.io/xmtp/backend:self-hosted"

[processes]
app = "--config-file /config.toml"

[[files]]
guest_path = "/config.toml"
local_path = "config.toml"

[http_service]
internal_port = 5050
force_https = true
auto_stop_machines = "off"
auto_start_machines = true
min_machines_running = 1
processes = ["app"]

[http_service.http_options]
h2_backend = true

[http_service.tls_options]
alpn = ["h2", "http/1.1"]

[checks.ready]
type = "tcp"
port = 5050
interval = "15s"
timeout = "5s"
grace_period = "120s"
processes = ["app"]

[[vm]]
cpu_kind = "shared"
cpus = 1
memory = "512mb"
```

`config.toml`:

```toml
#:schema https://raw.githubusercontent.com/xmtp/libxmtp/self-hosted/docs/schemas/backend-v1.json
[database]
url = "env:XMTP_DATABASE_URL"
```

Fly reads `local_path` at deploy time and writes the file at `guest_path`.
The process command supplies arguments to the image entrypoint. Use
`--config-file` because `/config.toml` is a path. See the
[configuration reference](/get-started/run-the-backend/#configuration) for other
keys, including authentication.

`h2_backend` makes Fly Proxy use h2c to the app. The ALPN list accepts native
gRPC over HTTP/2 and gRPC-Web over HTTP/1.1 at the TLS edge. Automatic stopping
is off to keep the app available for subscriptions.

Fly has no gRPC check type. The TCP check gives startup and migrations 120 s
before failures count. Increase this grace period if startup needs more time.
The 30 s `kill_timeout` covers the [shutdown budgets](/deploy/overview/#shutdown)
with a margin.

## Create the database and deploy

Run these commands from the directory that contains both files. Use the same
organization and region for the app and database:

```sh
fly config validate --strict
fly apps create my-xmtp-backend --org personal
fly mpg create --name my-xmtp-database --org personal \
  --region sjc --plan Basic --volume-size 10 --pg-major-version 17
```

Select PostgreSQL **17** explicitly. Fly Managed Postgres also offers 16, which
is below the backend's supported version.

In the cluster dashboard, open **Connect** and copy the **direct** connection
URL. Store it as the app secret `XMTP_DATABASE_URL`. Use the direct URL because
the backend needs session state for migrations. The default `fly mpg attach`
URL goes through PgBouncer. See
[Fly's client configuration](https://fly.io/docs/mpg/client-configuration/).

Import the secret through standard input. Enter `XMTP_DATABASE_URL=` followed by
the direct URL, then press Ctrl-D. Keep the URL out of both TOML files.

```sh
fly secrets import --app my-xmtp-backend
fly deploy --ha=false
```

`--ha=false` starts one Machine for this example. Before adding Machines, check
the [connection budget](/deploy/overview/#connection-budget).

Managed Postgres does not offer a read replica. If you need one, use an
unmanaged PostgreSQL deployment and follow the
[read replica requirements](/deploy/overview/#read-replica). Fly does not support
unmanaged PostgreSQL.

## Check the deployment

Use `https://my-xmtp-backend.fly.dev` as the SDK backend URL. Check gRPC health:

```sh
grpc-health-probe -addr=my-xmtp-backend.fly.dev:443 -tls
fly checks list --app my-xmtp-backend
```

## Verification record

The 2026-09-14 check used Fly CLI `0.4.102`. The guide's `fly.toml` passed
`fly config validate --strict`. Its `config.toml` passed JSON Schema validation
with `check-jsonschema 0.38.0`. Fly Managed Postgres **17.11**
(`Debian 17.11-1.pgdg13+2`) was provisioned successfully in `sjc`.

**Blocked:** the published `ghcr.io/xmtp/backend:self-hosted` image at digest
`sha256:594ce54f04379ed6fc86853019b687990ed397c0ee342d8a0861e6221d85b2fa`
rejected `--config-file` and exited with code 2. It predates the configuration
flag change. Fly injected the file and passed the process arguments, but the
backend did not start. The end-to-end exercise could not run. The health probe
timed out; the identity and trailer attempts received no backend response.

**UNTESTED:** successful gRPC health checks; `xdbg` identity registration, group
creation, message publish, and query readback; `grpcurl` trailers; and drain and
reconnect during a mid-stream restart. The guide will be exercised once an image
containing the new flag is published. It has not met the live deployment
verification requirement.

gRPC-Web, CORS, authentication, smart-contract wallets, read replicas, database
failover, and load capacity are also untested. The verification app and database
were destroyed after this attempt.
