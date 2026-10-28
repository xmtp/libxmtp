---
title: Deploy on Fly.io
description: Run the published XMTP backend image with Fly Proxy and Managed Postgres.
---

Fly injects your configuration file into the published image. No image build is
needed. Read the [deployment overview](/deploy/overview/) first for image tags,
ports, connection budgets, health, shutdown, ingress, and security requirements.

**Image requirement:** replace `sha-REPLACE_WITH_FULL_COMMIT` below with a real
`sha-<full commit>` tag. `--config-file` needs an image that contains the
configuration flag change, which merged at 2026-09-14 14:14 -0700, so an older
image exits at startup instead of deploying. The moving `:self-hosted` tag
would also change the running version under you on a later deploy.

## Create the configuration

Install [flyctl](https://fly.io/docs/flyctl/install/) and run `fly auth login`.
Create an empty directory. Save these two files in it. Replace
`my-xmtp-backend` with a unique app name. This example uses San Jose (`sjc`).

`fly.toml`:

```toml
app = "my-xmtp-backend"
primary_region = "sjc"
kill_signal = "SIGTERM"
kill_timeout = 30

[build]
image = "ghcr.io/xmtp/backend:sha-REPLACE_WITH_FULL_COMMIT"

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
#:schema https://raw.githubusercontent.com/xmtp/libxmtp/main/docs/schemas/backend-v1.json
[server]
# Change this to a reverse-DNS name you own. Never change it again once
# clients have connected: every client database is bound to it.
identifier = "org.example.xmtp"

[database]
url = "env:XMTP_DATABASE_URL"

# This app publishes an http_service, so anyone who reaches the URL can call
# publish, query, and subscription RPCs. Uncomment and set a key source to
# require bearer tokens before you carry real traffic.
# [auth]
# enabled = true
# jwks_url = "env:XMTP_JWKS_URL"
# audiences = ["xmtp"]
# issuers = ["https://auth.example.com"]
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
with a margin. Keep it. Fly's default is 5 s, which cuts the drain short. Fly
also treats `kill_timeout` as best effort.

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

This example uses the primary connection only. If you need a read replica,
check the current Managed Postgres options with Fly first. For an unmanaged
Fly Postgres (`fly postgres`) replica, follow the
[read replica requirements](/deploy/overview/#read-replica). Fly Support does
not cover unmanaged Fly Postgres, so you own its operations, upgrades, backups,
and disaster recovery.

## Charge callers by address

Backend images newer than `8.0.0-rc4` [limit each caller](/deploy/overview/#rate-limits),
and charge a caller without a credential to its address. The backend's peer is
Fly Proxy, so it must find the client in `X-Forwarded-For`. Fly
[documents](https://fly.io/docs/networking/request-headers/) that this header
lists the client that originated the request and that "the last address
(rightmost) in this list will be a shared or dedicated IP address assigned to
your app". After the first deploy, list those addresses:

```sh
fly ips list --app my-xmtp-backend
```

Add both to `config.toml` with the network Fly Proxy connects from, then run
`fly deploy --ha=false` again. Replace the example addresses:

```toml
[rate_limits]
# Fly Proxy's private network, then this app's public IPv4 and IPv6.
trusted_proxies = ["172.16.0.0/12", "66.241.125.1/32", "2a09:8280:1::1:0/128"]
```

The backend reads the list from the right, skips these addresses, and charges
the first address it does not trust: the client as Fly Proxy saw it. An entry
a client wrote itself sits further left and is never chosen. Fly does not
document the network Fly Proxy connects from. `172.16.0.0/12` is what Fly apps
report; this guide has not verified it. If the proxy connects from elsewhere,
the backend ignores the header and every caller without a credential shares
the proxy's allowance. That is never more permissive. The backend does not
read `Fly-Client-IP`.

## Check the deployment

Use `https://my-xmtp-backend.fly.dev` as the SDK backend URL. Check gRPC health:

```sh
grpc-health-probe -addr=my-xmtp-backend.fly.dev:443 -tls
fly checks list --app my-xmtp-backend
```

The backend serves no gRPC reflection service. `grpcurl` therefore needs a
local copy of the service definition, for example
`grpcurl -import-path . -proto health.proto`. Without it, `grpcurl` fails while
it resolves descriptors, even though the endpoint is healthy.
