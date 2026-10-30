---
title: Deployment overview
description: Deploy on any host with PostgreSQL, a server, a configuration file, and TLS termination.
---

The backend is one binary with no durable local state. Run one instance or
several instances behind a load balancer. All instances use the same PostgreSQL
primary. A client can reconnect to another instance with its safe topic cursors.

## Required components

| Component       | Requirement                                                                                                                             |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| Database        | PostgreSQL 17 or later with persistent storage. No extensions are required.                                                             |
| Server          | A host that can run the backend binary or its Linux `amd64` or `arm64` container. The process must stay running to serve subscriptions. |
| TLS termination | A hostname and a valid certificate at a trusted reverse proxy or load balancer. Forward traffic to the private backend listener.        |
| Configuration   | One TOML document with `server.identifier` and `database.url`. Use the same identifier on every instance.                               |

The backend needs no persistent application volume. Back up PostgreSQL; it holds
the durable state. Client apps have separate local databases that they must
preserve. Size the server for its connections and traffic; the backend defines
no fixed CPU or memory minimum.

For remote ciphertext uploads, configure an optional
[attachment storage target](/deploy/attachment-storage/).

## Configuration file

Mount this file and start the binary with
`xmtp-backend --config-file /path/to/config.toml`. For the container, pass only
`--config-file /path/to/config.toml` after the image name.

```toml
[server]
identifier = "org.example.xmtp"
listen = "0.0.0.0:5050"

[database]
url = "env:XMTP_DATABASE_URL"
```

Set `XMTP_DATABASE_URL` to the primary's PostgreSQL connection string. The
database role must be able to apply schema migrations and read and write
application data. Replace the example identifier with a name you control.
Keep it unchanged after clients connect, including when moving the deployment.

Add `sslmode=verify-full` to the connection string so the backend checks the
database's certificate and host name. If the provider's CA is not a public
root, also set `sslrootcert` to its CA file. `require` accepts any certificate,
and `verify-ca` checks the certificate chain but not the host name; only
`verify-full` checks both. Unless the log level is `error` or `off`, the
backend logs a warning at startup for a remote store in any other mode, for
disabled auth on a listener that is not loopback, and for a plain `http://`
chain URL to a host that is not loopback.

Supply exactly one configuration source: `--config-file`, inline `--config`,
or inline TOML in `XMTP_CONFIG`. Environment references resolve at startup.
Missing variables and invalid configuration stop startup. Restart instances to
apply changes. See [Run the backend](/get-started/run-the-backend/#configuration)
for all settings and a container command.

## Choose a platform

These ingress mechanisms must meet the transport requirements below. How far
each path has been exercised differs by platform, so check a guide's own
caveats, and the ingress contract below, before you rely on it.

| Platform                                                      | Ingress mechanism                                                     | Native gRPC                                                                         |
| ------------------------------------------------------------- | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| [Local Linux or Mac with Tailscale](/deploy/local-tailscale/) | Tailscale Serve with private HTTPS and h2c to the backend             | Supported by current stable Tailscale Serve                                         |
| [Fly.io](/deploy/fly/)                                        | Fly Proxy with TLS termination and h2c to the backend                 | Yes                                                                                 |
| [Railway](/deploy/railway/)                                   | **DEVELOPMENT ONLY:** TCP proxy to a separate HAProxy TLS terminator  | Yes, through the TCP proxy and HAProxy. The HTTP edge does not support native gRPC. |
| [AWS ECS (Fargate)](/deploy/aws-ecs/)                         | NLB TLS listener with ALPN `HTTP2Preferred` and a TCP target group    | Yes                                                                                 |
| [Kubernetes](/deploy/kubernetes/)                             | Gateway API HTTPS listener and HTTPRoute, with an h2c backend service | Yes, with a controller that preserves streaming and trailers                        |

## Container image

Pull `ghcr.io/xmtp/backend` without registry credentials. The image is public.

- `:self-hosted` moves with the `main` branch.
- `:sha-<commit>` is the immutable tag for a full commit SHA. Use it to pin a deployment.
- The publish workflow builds Linux `amd64` and `arm64` images and publishes both tags as manifest lists. The container runtime selects the matching image.

## Database and migrations

Use PostgreSQL 17 or later. No extensions are required.

Migrations run in the backend process against the primary at boot, before the
RPC listener binds. There is no separate migration job.

Each schema change adds a new migration. There is no supported in-place
upgrade path yet, so a schema change can require a fresh database. Back up
the database before deployment. Startup never deletes an existing database.

After the backend stores a handle claim, do not roll back to a backend build
that predates handles. That build cannot read a stored claim, so it rejects
every later identity update of each inbox that claimed a handle. The minimum
name length is the `handles.min_name_length` setting; see
[Run the backend](/get-started/run-the-backend/#handles) and
[Handle limits](/reference/limits/#handle-limits).

### Upgrade to keyed group ids

The release that introduces keyed group ids changes what the backend accepts.
A new group's id is a 32-byte public key, and the backend stores a group
message or a commit-log entry only when it carries a valid signature under a
key that only the group's members hold. Group and commit-log topics carry
32-byte identifiers.

The upgraded backend refuses these requests from older clients:

- A publish of a group message whose group id is 16 bytes fails with
  `REASON_MALFORMED_PAYLOAD`.
- A publish of a commit-log entry in the retired unsigned form fails with
  `REASON_MALFORMED_PAYLOAD`.
- A read, stream update, or push subscription that names a 17-byte group or
  commit-log topic fails with `INVALID_ARGUMENT`.

Envelopes stored under 17-byte topics stay in the database, but no request can
read them. Push subscriptions stored under 17-byte topics also stay. An update
that names one in its removes fails as a whole, and these rows still count
toward `max_push_topics`. They go away only when the installation unregisters
or its registration expires.

Upgrade in this order:

1. Back up PostgreSQL, then upgrade the backend.
2. Set `server.min_libxmtp_version` to the libxmtp version of the first SDK
   release with keyed group ids. See
   [`[server]`](/get-started/run-the-backend/#server).

An older client learns the minimum from the published configuration. A client
created on a new database, or on one that last connected to another backend
URL, fetches it first and refuses to build. A client created on a database
that already holds a stored configuration builds from that copy, and learns
the minimum at its first refresh, about an hour after it starts. A running
client stops at that refresh.

Until then, the backend refuses an older client's group publishes as
described above, and the client treats each refusal as final. Its other
requests that name a 17-byte topic fail too, and some repeat: an older
client's commit-log reader sends a refused read about once a second.

Do not upgrade the SDKs before the backend. A new client does not detect an
older backend, and an older backend refuses its group messages and commit-log
entries. For the same reason, do not roll the backend back to an earlier
release after clients with keyed group ids have connected: an earlier backend
routes only 16-byte group ids, so it refuses every keyed group's messages and
reads.

### Connection budget

Without a replica, each instance uses up to `max_connections` pooled connections
and one dedicated tailer connection. With the default settings, that is 21.
With a replica, budget 20 pooled connections on the primary and 21 on the replica
at the defaults. When push delivery is enabled, its dispatcher also holds one
dedicated primary connection. Add that connection to the primary budget, plus
capacity for migrations and administrative connections.

### Read replica

A read replica is optional. If you configure one, it must be a single physical
replica, not a reader endpoint that load-balances across independently lagging
replicas. Reads served by the replica can lag behind a successful publish. See
[`[database]`](/get-started/run-the-backend/#database) for both settings.

## Ports and health

| Default port | Purpose                                     | Exposure                                                                                                  |
| ------------ | ------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `443`        | HTTPS at your TLS terminator                | Public client entry point. Advertise `h2` and `http/1.1` through TLS ALPN.                                |
| `5050`       | Native gRPC, gRPC-Web, and `grpc.health.v1` | Public application traffic through a trusted TLS terminator. Keep the plaintext backend listener private. |
| `5432`       | PostgreSQL                                  | Private access from backend instances. Use the database's configured port if different.                   |
| `9464`       | Prometheus `/metrics`                       | Private monitoring only.                                                                                  |

The default listen addresses are `0.0.0.0:5050` and `0.0.0.0:9464`. A bind to all
interfaces does not restrict access. Use network controls to keep metrics private.
`[auth]` does not protect the metrics listener. Port 9464 stays unauthenticated
even when authentication is on, so never publish it. See
[telemetry configuration](/get-started/run-the-backend/#telemetry).

Probe health through `grpc.health.v1` only. There is no HTTP health path.
Shutdown marks aggregate health and every named RPC service `NOT_SERVING`.
For example, run `grpc-health-probe -addr=127.0.0.1:5050` on the backend host.
The listener starts after database initialization and migrations succeed.

## Shutdown

On SIGTERM, the backend stops request admission and ends active subscriptions.
Admitted unary requests can finish within `server.max_drain_duration_ms`.
Its default is `10000` ms, or 10 s. At the deadline, the backend cancels remaining
handlers and connection I/O.

After the drain, telemetry has a separate 5 s flush budget. Set the platform
termination grace period to cover both budgets, plus any platform stop delay
and a margin. Clients must reconnect after shutdown. A dropped publish response
does not prove that the write failed.

## Ingress contract

Use TLS on every public endpoint that carries application traffic. Terminate
TLS at a trusted load balancer and forward to the private backend.

The TLS terminator must pass HTTP bytes through without gRPC conversion or
buffering. It must speak h2c (HTTP/2 without TLS) or HTTP/1.1 to the backend.
Both native gRPC clients over HTTP/2 and gRPC-Web clients over HTTP/1.1 must work.
An ingress path that downgrades native gRPC to HTTP/1.1 does not meet this contract.

Configure HTTP/2 (h2c) for the upstream connection used by native gRPC. Browser
gRPC-Web can also use that upstream; it needs no conversion proxy. Keep each
open stream on its serving instance. Reconnects need no sticky sessions.

Disable request and response buffering for streams. Set stream idle timeouts
above the configured application keepalive interval (30 seconds by default),
with a margin. Allow long-lived responses. HTTP/2 connection PINGs do not reset
every proxy's per-stream timeout. Check both idle and maximum request-duration
limits on your load balancer.

- Forward `access-control-request-headers` unmodified. The backend mirrors the
  preflight request, so this header is input. A terminator that strips or
  rewrites it breaks browser clients. The backend mirrors the requested header list, including `authorization`,
  `content-type`, `x-app-version`, `x-libxmtp-version`, `traceparent`, and
  `tracestate` when the client requests them.
- Forward response frames as the backend emits them. A subscription must receive
  its Started frame and later messages while the same response stays open.
- Preserve `grpc-status`, `grpc-message`, and `grpc-status-details-bin` trailers,
  including trailers encoded in gRPC-Web response bodies.
- Preserve `access-control-expose-headers`, which carries `grpc-status`,
  `grpc-message`, `grpc-status-details-bin`, and `x-request-id`. Browser clients
  cannot read status details without it. The backend sets no
  `access-control-max-age`, so do not enable preflight caching at the edge.

Check your ingress against both client kinds before you rely on it. A path that
serves unary calls correctly can still drop long-lived subscriptions or strip
the trailers that carry error details.

## Rate limits

The backend limits the requests each caller can make. Rate limits are in
backend releases after `8.0.0-rc4`, and in `:self-hosted` and `sha-` images
built from a commit that includes them. An older image charges nothing and
refuses a `[rate_limits]` table at startup, naming `rate_limits` as "not a
configuration key"; start the image you plan to deploy with your configuration
before you deploy it. Limits are on by default, with values sized well above
one client's normal use; `rate_limits.enabled = false` turns them off. See
[`[rate_limits]`](/get-started/run-the-backend/#rate_limits) for every key, and
[RATE](/specs/rate-backend-rate-limits/) for the exact rules.

Each request is charged before the backend does the work, in these categories:

| Category          | What one unit is                                     | Default limits                   |
| ----------------- | ---------------------------------------------------- | -------------------------------- |
| `request`         | One RPC, except health checks and `GetConfiguration` | 6000 per minute, 200000 per hour |
| `publish`         | One published envelope                               | 1000 per minute, 20000 per hour  |
| `identity_update` | One published identity update                        | 10 per minute, 100 per hour      |
| `chain_verify`    | One smart contract wallet signature to verify        | 200 per minute, 2000 per hour    |
| `push`            | One topic added to a push subscription               | 2000 per minute, 20000 per hour  |
| `upload`          | One attachment upload request                        | 120 per minute, 2000 per hour    |

A limit admits a burst of its count, then its count per window. A refused
request gets `RESOURCE_EXHAUSTED` with the category and a retry delay: the
time after which the refused charge fits, if the caller spends nothing more in
that category meanwhile. SDK clients wait that delay before they send the
request again and never split it into smaller requests. The refused charge is not
counted; charges the request already passed, such as its `request` unit, are.

**Older clients.** An SDK client from before rate limits reads
`RESOURCE_EXHAUSTED` as a request that is too large. It splits a refused read
or publish batch into smaller requests at once. Those are refused too, or store
part of a batch the limit refused as a whole, and each spends more of the
caller's allowance. Before you turn limits on, set `server.min_libxmtp_version`
to the libxmtp version of the first SDK release with rate limits, or set
`rate_limits.enabled = false` until your clients have upgraded. See
[`[server]`](/get-started/run-the-backend/#server).

**Callers.** The backend charges a request to the most specific identity it
has: the API key's name, else a JWT's issuer and subject, else the client
address. A JWT without `sub` is charged to its address. An IPv6 address is
charged by its /64, because one subscriber holds the whole /64.

**Client address.** By default the client address is the peer of the
connection. Behind a proxy or load balancer, that peer is the proxy, and every
caller without a credential shares the proxy's allowance. To charge each client
to its own address, make the proxy append the address it received the request
from to `X-Forwarded-For`, and list the proxy's network in
`rate_limits.trusted_proxies`. When the peer is a listed proxy, the backend
reads `X-Forwarded-For` from the right, skips listed proxies, and takes the
first address it does not trust. Never list a proxy that passes a client's
`X-Forwarded-For` through unchanged: a client could then choose the address it
is charged to. A load balancer that keeps the client's address on the
connection needs no list.

**Shared addresses.** Callers without a credential behind one address, such as
an office, a campus, or a carrier-grade NAT, share one allowance in every
category. If your users share addresses, give them credentials, or raise the
`ip` tier of each category they reach. Each new installation publishes an
identity update, so `identity_update` caps how many installations one address
can register: 10 a minute and 100 an hour by default.

```toml
[rate_limits.categories.request]
ip = [{ count = 30000, window_seconds = 60 }, { count = 1000000, window_seconds = 3600 }]

[rate_limits.categories.publish]
ip = [{ count = 5000, window_seconds = 60 }, { count = 100000, window_seconds = 3600 }]

[rate_limits.categories.identity_update]
ip = [{ count = 100, window_seconds = 60 }, { count = 2000, window_seconds = 3600 }]

[rate_limits.categories.push]
ip = [{ count = 10000, window_seconds = 60 }, { count = 100000, window_seconds = 3600 }]

[rate_limits.categories.upload]
ip = [{ count = 600, window_seconds = 60 }, { count = 10000, window_seconds = 3600 }]
```

An override raises or lowers the limits of one API key, one JWT subject, or one
address. An empty list of limits makes that category unlimited.

**Per instance.** Each instance keeps its own counts in memory. A deployment
of N instances admits up to N times a caller's limits when its requests spread
across them. A restart forgets every caller's spending. The counts take
`rate_limits.max_memory_mib` of memory, 100 MiB by default, allocated at
startup; include it when you size the instance. On 2026-10-29 the backend
image measured about 115 MiB resident at startup with the defaults, and about
12.5 MiB with `rate_limits.enabled = false`. When the table is full, the
backend forgets the callers that have spent least rather than refuse anyone.
A rising `xmtp_rate_limit_evictions_total` means it is too small.
`xmtp_rate_limit_rejections_total` counts refusals by category and auth type.

How each guide's ingress reaches the backend:

| Platform                                                                  | Client address the backend sees                                                             |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| [Local Linux or Mac with Tailscale](/deploy/local-tailscale/#rate-limits) | None: callers without a credential share one allowance                                      |
| [Fly.io](/deploy/fly/#charge-callers-by-address)                          | Fly Proxy appends it to `X-Forwarded-For`; list Fly's proxy network and the app's addresses |
| [Railway](/deploy/railway/#rate-limits)                                   | None: callers without a credential share one allowance                                      |
| [AWS ECS (Fargate)](/deploy/aws-ecs/#client-addresses)                    | The connection carries it, with client IP preservation on the target group                  |
| [Kubernetes](/deploy/kubernetes/#client-addresses)                        | Envoy appends it to `X-Forwarded-For`; list the pod network                                 |

**Upgrading an existing deployment.** A deployment built from an earlier
version of these guides has no `[rate_limits]` table and, on ECS, no client IP
preservation. An image with rate limits turns them on, so behind a proxy every
caller without a credential would share the proxy's allowance. Before you
upgrade:

- On AWS ECS, enable client IP preservation on the target group, as the guide
  now does. An image without rate limits is unaffected by it.
- On Fly.io and Kubernetes, add the guide's `trusted_proxies` in the same
  deploy that moves to an image with rate limits. An older image refuses the
  table, so the configuration and the image change together.
- Behind any other proxy, in that same deploy, list the proxy in
  `trusted_proxies` if it appends the client's address, or raise the `ip`
  tier, or set `rate_limits.enabled = false` until you have.

## Security

Optional API key and JWT authentication is available. Configure
[`[auth]`](/get-started/run-the-backend/#auth) to require valid bearer tokens.
[Rate limits](#rate-limits) bound the requests each caller can make.
A valid token does not prove group membership.

Without `[auth]`, the service is unauthenticated. Do not expose an unauthenticated
service to untrusted traffic. Use a private network or restrict access at a
trusted load balancer. TLS protects transport; it does not restrict callers.

## Optional integrations

Configure [push delivery](/deploy/push-configuration/) if your apps use
APNs, FCM, or HTTPS notifications. Permit outbound access to the configured
providers. JWT key discovery needs outbound HTTPS to the JWKS URL.
Smart contract wallet verification needs outbound access to the RPC URLs in
`[chains]`. These services are not required when those features are unused.
