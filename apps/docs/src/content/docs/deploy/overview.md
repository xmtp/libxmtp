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

Supply exactly one configuration source: `--config-file`, inline `--config`,
or inline TOML in `XMTP_CONFIG`. Environment references resolve at startup.
Missing variables and invalid configuration stop startup. Restart instances to
apply changes. See [Run the backend](/get-started/run-the-backend/#configuration)
for all settings and a container command.

## Choose a platform

These ingress mechanisms must meet the transport requirements below. How far
each path has been exercised differs by platform, so check a guide's own
caveats, and the ingress contract below, before you rely on it.

| Platform                              | Ingress mechanism                                                     | Native gRPC                                                                         |
| ------------------------------------- | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| [Fly.io](/deploy/fly/)                | Fly Proxy with TLS termination and h2c to the backend                 | Yes                                                                                 |
| [Railway](/deploy/railway/)           | **DEVELOPMENT ONLY:** TCP proxy to a separate HAProxy TLS terminator  | Yes, through the TCP proxy and HAProxy. The HTTP edge does not support native gRPC. |
| [AWS ECS (Fargate)](/deploy/aws-ecs/) | NLB TLS listener with ALPN `HTTP2Preferred` and a TCP target group    | Yes                                                                                 |
| [Kubernetes](/deploy/kubernetes/)     | Gateway API HTTPS listener and HTTPRoute, with an h2c backend service | Yes, with a controller that preserves streaming and trailers                        |

## Container image

Pull `ghcr.io/xmtp/backend` without registry credentials. The image is public.

- `:self-hosted` moves with the `self-hosted` branch.
- `:sha-<commit>` is the immutable tag for a full commit SHA. Use it to pin a deployment.
- The publish workflow builds Linux `amd64` and `arm64` images and publishes both tags as manifest lists. The container runtime selects the matching image.

## Database and migrations

Use PostgreSQL 17 or later. No extensions are required.

Migrations run in the backend process against the primary at boot, before the
RPC listener binds. There is no separate migration job.

The backend currently ships a single migration that can change between
releases. **There is no in-place upgrade path yet,** so a schema change can
require a fresh database. Startup never deletes an existing database.

### Connection budget

Without a replica, each instance needs up to `max_connections + 1` connections:
its request pool plus one for the tailer. The default is 21. With a replica,
budget 20 on the primary and 21 on the replica per instance at the default
settings. Add capacity for migrations and administrative connections.

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
  rewrites it breaks browser clients. The backend allows `authorization`,
  `content-type`, `x-app-version`, `x-libxmtp-version`, `traceparent`, and
  `tracestate`.
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

## Security

Optional API key and JWT authentication is available. Configure
[`[auth]`](/get-started/run-the-backend/#auth) to require valid bearer tokens.
Caller quotas are not implemented. A valid token does not prove group membership.

Without `[auth]`, the service is unauthenticated. Do not expose an unauthenticated
service to untrusted traffic. Use a private network or restrict access at a
trusted load balancer. TLS protects transport; it does not restrict callers.

## Optional integrations

Configure [push delivery](/get-started/push-configuration/) if your apps use
APNs, FCM, or HTTPS notifications. Permit outbound access to the configured
providers. JWT key discovery needs outbound HTTPS to the JWKS URL.
Smart contract wallet verification needs outbound access to the RPC URLs in
`[chains]`. These services are not required when those features are unused.
