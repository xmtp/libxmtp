---
title: Deploy on Railway
description: Evaluate the XMTP backend through a Railway TCP proxy and a separate HAProxy TLS service.
---

**DEVELOPMENT ONLY:** This example has no caller authentication. TLS protects
traffic, but anyone who can reach the endpoint can call the backend. Use only
disposable evaluation data. Configure [authentication](/get-started/run-the-backend/#auth)
before use with untrusted traffic. Caller quotas are not implemented.

Run two application services in one Railway project: public `haproxy` and
private `backend`. Add `Postgres` for storage. HAProxy terminates TLS with a
publicly trusted certificate. Railway's TCP proxy forwards raw packets to it.
The backend receives h2c over Railway's private network. Read the
[deployment overview](/deploy/overview/) for database connection budgets,
health, shutdown, and the ingress contract.

Clients must use the Railway-assigned high port, **not port 443**. Do not create
an HTTP domain for either service. Railway staff state that the HTTP edge
“will demux down to HTTP/1.1 thus breaking gRPC”. That edge also closes requests
after 15 minutes, even with data flowing. The TCP proxy has no such documented
request limit. See [TCP proxy setup](https://docs.railway.com/networking/tcp-proxy)
and [public networking limits](https://docs.railway.com/networking/public-networking/specs-and-limits).

## Create the backend configuration

Use a scratch directory outside the repository. Save this as `config.toml`:

```toml title="config.toml"
#:schema https://raw.githubusercontent.com/xmtp/libxmtp/self-hosted/docs/schemas/backend-v1.json
[server]
listen = "[::]:5050"

[database]
url = "env:XMTP_DATABASE_URL"
```

`[::]:5050` accepts IPv4 and IPv6. `0.0.0.0:5050` accepts only IPv4 and fails
with legacy Railway private DNS. Environments created after 2025-10-16 resolve
both address families; older environments resolve only IPv6. This setting
controls the **inbound listener**. Outbound database connectivity is separate.
See [private network addressing](https://docs.railway.com/networking/private-networking/how-it-works).

Set `XMTP_CONFIG` to this complete inline TOML document. Do not set it to a
file path or also supply `--config-file`. Put sensitive values in separate
sealed variables and use `env:NAME` references in the document. Process
environments can be read through `/proc/<pid>/environ`, crash reporters, and
platform consoles. Sealing limits platform access; it does not hide a secret
from the process that needs it.

## Create Postgres and the services

In the Railway dashboard:

1. Create a project with a unique name. Keep all services in one environment
   and region. Start with one replica of each application service.
2. Add the **Postgres** template. It runs PostgreSQL 18, which meets the
   [database requirements](/deploy/overview/#database-and-migrations).
3. Add a service named `backend` from this published image:

   ```text
   ghcr.io/xmtp/backend:sha-7871fa5ee0da8e9cf37596744efd3330f6540c1a
   ```

4. In `backend` **Variables**, set `XMTP_CONFIG` to the document above. Set
   `XMTP_DATABASE_URL` to `${{Postgres.DATABASE_URL}}` and seal it. This is the
   private-network URL. Do not use `DATABASE_PUBLIC_URL`: it sends database
   traffic through the public proxy and can incur egress charges.
5. Leave the backend start command unset. The image reads `XMTP_CONFIG`.
   Leave **Public Networking** empty: no HTTP domain and no TCP proxy.
   Remove the Postgres template's public TCP proxy if it is present.
6. Deploy the backend. Migrations run before the RPC listener starts. Check
   logs for startup errors. Do not configure an HTTP health path; health uses
   `grpc.health.v1`.

The pinned image supports both Linux `amd64` and `arm64`. Deploy it directly.
No backend change, TLS configuration key, or derived backend image is needed.
`rustls` and `tokio-rustls` remain optional and gated by test utilities.

Port 9464 is absent from the public networking configuration and HAProxy
configuration. The backend's default metrics listener remains private. Omitting
`[telemetry]` does not disable that listener. Never create a public proxy for it.

## Prepare the HAProxy service

Save the following files in a separate `haproxy` directory. The configuration is
an exact copy of [`dev/tls/haproxy.cfg`](https://github.com/xmtp/libxmtp/blob/self-hosted/dev/tls/haproxy.cfg).
Its existing `resolve-prefer ipv6` supports legacy `*.railway.internal` DNS.
No routing or timeout setting changes for Railway. The local certificate
comment describes the Task 7 testing and validation stack only. This deployment
uses a publicly trusted DNS-01 certificate.

```text title="haproxy.cfg"
# Set XMTP_TLS_PEM to an absolute PEM path and XMTP_TLS_BACKEND to host:port.
# Forward the host's public TCP port to this listener on port 18443.
global
    log stdout format raw local0

defaults
    mode http
    log global
    option httplog
    timeout connect 5s
    # HTTP/2 streams use client/server timers, not timeout tunnel.
    # Connection-level gRPC PING frames do not refresh these stream timers.
    timeout client 24h
    timeout server 24h
    timeout http-request 10s
    timeout http-keep-alive 5s
    # MUST NOT add option http-buffer-request: it delays streaming requests.
    # MUST NOT add http-drop-request-trailers: it discards gRPC metadata.
    # MUST NOT add http-drop-response-trailers: it discards gRPC status/details.

resolvers system
    parse-resolv-conf
    resolve_retries 3
    timeout resolve 1s
    timeout retry 1s
    hold valid 10s

frontend tls
    # The generated self-signed certificate is for testing and validation only.
    # Supply a trusted certificate plus private key in one PEM for deployment.
    bind :18443 ssl crt "${XMTP_TLS_PEM}" alpn h2,http/1.1
    default_backend xmtp

backend xmtp
    # One h2c upstream serves native gRPC and HTTP/1.1 gRPC-Web requests.
    # Prefer AAAA records for legacy *.railway.internal environments.
    server backend "${XMTP_TLS_BACKEND}" proto h2 resolvers system init-addr last,libc,none resolve-prefer ipv6
```

One frontend and one `proto h2` backend serve both client kinds. Task 7 verified
native gRPC and HTTP/1.1 gRPC-Web through this same backend, without a routing
split. Its [`dev/tls/check.py`](https://github.com/xmtp/libxmtp/blob/self-hosted/dev/tls/check.py)
also checks trailers, CORS, and incremental delivery.

**Keep both 24 h timeouts.** `timeout tunnel` does not apply to HTTP/2.
Connection-level gRPC PING frames do not refresh an active stream's timer.
The backend's 30 s keepalive therefore does not keep an idle subscription alive
through HAProxy. Task 7's deliberate short-timeout run dropped an idle
subscription after about 12.5 s with 12 s client/server timeouts, despite
keepalives. Keep `http-request` and `http-keep-alive` short. Clients still need
to reconnect after the long timeout or a deployment restart.

```dockerfile title="Dockerfile"
FROM goacme/lego:v4.35.2 AS acme
FROM haproxy:3.2-alpine
USER root
COPY --from=acme /lego /usr/local/bin/lego
COPY haproxy.cfg /usr/local/etc/haproxy/haproxy.cfg
COPY start.sh install-certificate.sh /usr/local/bin/
ENTRYPOINT ["/bin/sh", "/usr/local/bin/start.sh"]
```

```sh title="start.sh"
#!/bin/sh
set -eu
# Keep the container available for SSH during initial certificate issuance.
while [ ! -s "$XMTP_TLS_PEM" ]; do
    sleep 2
done
exec haproxy -W -db -p /run/haproxy.pid \
    -f /usr/local/etc/haproxy/haproxy.cfg
```

```sh title="install-certificate.sh"
#!/bin/sh
set -eu
umask 077
pem="$XMTP_TLS_PEM"
cat "/certs/acme/certificates/$CERT_DOMAIN.crt" \
    "/certs/acme/certificates/$CERT_DOMAIN.key" > "$pem.new"
XMTP_TLS_PEM="$pem.new" haproxy -c -f /usr/local/etc/haproxy/haproxy.cfg
mv "$pem.new" "$pem"
if [ -s /run/haproxy.pid ]; then
    kill -USR2 "$(cat /run/haproxy.pid)"
fi
```

The `.crt` file from lego includes the certificate chain. The script combines
it with the private key, validates the new PEM, then replaces the old PEM
atomically. `SIGUSR2` reloads the HAProxy master started with `-W`. New
connections use the new certificate. Old workers can finish existing streams.
A restart also reads the PEM from disk, but interrupts active subscriptions.

Create an empty service named `haproxy`. Add a Railway **volume** at
`/certs` before deployment. Keep both `/certs/acme` and `/certs/server.pem` on
that volume. The ACME account, certificate state, and PEM must survive redeploys.
Never copy a private key into the image or commit it to a repository.

Set these variables on `haproxy`:

```text
XMTP_TLS_PEM=/certs/server.pem
XMTP_TLS_BACKEND=${{backend.RAILWAY_PRIVATE_DOMAIN}}:5050
CERT_DOMAIN=xmtp.example.com
ACME_EMAIL=operator@example.com
```

Replace the domain and email with values you control. Deploy with the Railway
CLI from the `haproxy` directory. Replace `PROJECT_NAME` with this project's
name. Use its environment name if it is not `production`:

```sh
railway link --project PROJECT_NAME --environment production --service haproxy
railway up --service haproxy --detach
railway deployment list --service haproxy --json
```

Wait for the submitted deployment to report `SUCCESS`. Before the first PEM is
installed, the container waits and the TLS listener is not yet available.

## Obtain and renew the certificate

Use **DNS-01** with lego or certbot. This example uses lego manual mode and
requires access to your DNS zone. It does not need a DNS API token.

Open a shell in the HAProxy service:

```sh
railway ssh --service haproxy -- /bin/sh
```

Run these commands inside that shell:

```sh
lego --path /certs/acme --email "$ACME_EMAIL" --accept-tos \
    --domains "$CERT_DOMAIN" --dns manual run
sh /usr/local/bin/install-certificate.sh
```

Lego prints the exact `_acme-challenge` TXT record name and value, then waits.
Create that record at your DNS provider. Wait until authoritative DNS serves
it, then continue lego. Remove the challenge record when lego tells you to.
The installer starts the first TLS listener after the PEM is ready.

**Renewal is required.** Check expiry daily and renew with at least 30 days
remaining. Run this inside the same service, with the same volume:

```sh
lego --path /certs/acme --email "$ACME_EMAIL" --accept-tos \
    --domains "$CERT_DOMAIN" --dns manual renew --days 30
sh /usr/local/bin/install-certificate.sh
```

Manual renewal requires an operator to create each new TXT challenge. Set an
expiry alert and assign an owner; this mode cannot renew unattended. For
unattended renewal, use lego's DNS provider integration or certbot's DNS plugin
with a narrowly scoped, sealed DNS credential. Schedule a daily renewal check
in the service that mounts `/certs`. Run the installer only after successful
renewal. A separate Railway service cannot share this mounted volume.

Confirm the new certificate serial and expiry through the public endpoint after
each renewal. Also restart the service once and confirm it serves the same
certificate from the volume. Do not force a new issuance on every startup.

Do not use HTTP-01. Railway's edge answers port 80 with a synthesized 301 before
any upstream lookup, so it cannot route the challenge to this container.
Do not use HAProxy 3.2's native ACME. It is experimental, lacks TLS-ALPN-01,
and keeps certificates in memory without writing them to disk. Each redeploy
would re-issue a certificate and can quickly hit Let's Encrypt's limit of five
certificates for the same identifiers per week. See
[HAProxy ACME](https://www.haproxy.com/documentation/haproxy-configuration-tutorials/security/ssl-tls/lets-encrypt/)
and [Let's Encrypt rate limits](https://letsencrypt.org/docs/rate-limits/).

## Enable the TCP proxy and DNS

In `haproxy` **Settings → Networking**, choose **TCP Proxy** and enter internal
port `18443`. Copy the generated proxy hostname and high port. Do not choose
**Generate Domain** or add an HTTP custom domain. CLI 4.58.0 has no TCP proxy
subcommand; `railway domain --port` creates an HTTP domain and is not a substitute.

At your DNS provider, create a CNAME from your certificate domain to the
Railway TCP proxy hostname, without the port. For Cloudflare, select **DNS only**
(grey cloud). Clients use `https://xmtp.example.com:ASSIGNED_PORT`.
HAProxy, not Railway or DNS, terminates TLS on this path.

Do not enable Cloudflare's HTTP proxy in front of this endpoint:

- Cloudflare's gRPC origin must use port 443, TLS, HTTP/2 over ALPN, and at
  least Full mode. Railway assigns a high TCP proxy port and provides no TLS
  termination there. A Full-mode TLS fetch to a plaintext backend fails with 525. HAProxy supplies TLS in this guide, but does not remove the port-443
  requirement or map Cloudflare's origin fetch to the assigned port.
- Railway's TCP proxy documentation explicitly requires grey-cloud DNS.
  DNS-only records terminate no TLS and apply no HTTP proxy protection.
- Cloudflare's 125 s proxy read timeout can cut an idle subscription with 524.
  Only Enterprise can raise it. Activity on other HTTP/2 streams is not a
  substitute for subscription data.

Cloudflare Spectrum's generic TCP support requires Enterprise plus an add-on.
Cloudflare Tunnel supports gRPC through private subnet routing, not public
hostnames. Neither is a substitute for this public endpoint. See
[Cloudflare gRPC requirements](https://developers.cloudflare.com/network/grpc-connections/),
[524 errors](https://developers.cloudflare.com/support/troubleshooting/http-status-codes/cloudflare-5xx-errors/error-524/),
[Spectrum availability](https://developers.cloudflare.com/spectrum/), and
[Tunnel gRPC support](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/private-net/cloudflared/).

## Check the deployment

Use the certificate hostname and assigned port. Do not disable certificate
verification or add a custom trust certificate. For example:

```sh
grpc-health-probe -addr=xmtp.example.com:ASSIGNED_PORT -tls
```

Build xdbg from this repository with `nix build .#xdbg`. In a repository devshell,
set `XDBG_DB_ROOT` to an empty scratch directory and run:

```sh
export XDBG_DB_ROOT=/tmp/xmtp-railway-client
export XMTP_ENDPOINT=https://xmtp.example.com:ASSIGNED_PORT
./result/bin/xdbg --url "$XMTP_ENDPOINT" --fail-fast generate --entity identity --amount 2
./result/bin/xdbg --url "$XMTP_ENDPOINT" --fail-fast generate --entity group --amount 1 --invite 2
./result/bin/xdbg --url "$XMTP_ENDPOINT" --fail-fast generate --entity message --amount 10
./result/bin/xdbg --url "$XMTP_ENDPOINT" --fail-fast sync
./result/bin/xdbg --url "$XMTP_ENDPOINT" --fail-fast query all-key-packages
```

Export the generated group's topic and make wire-format requests. xdbg exports
topics as hex strings; protobuf JSON requires base64:

```sh
./result/bin/xdbg --url "$XMTP_ENDPOINT" export --entity group-topics --out group-topics.json
python3 - <<'PYTHON'
import base64
import json
from pathlib import Path

topics = json.loads(Path("group-topics.json").read_text())
queries = [{"topic": {"topic": base64.b64encode(bytes.fromhex(t)).decode()}}
           for t in topics]
Path("query.json").write_text(json.dumps({"queries": queries, "limit": 1000}))
Path("subscription.json").write_text(json.dumps({"topics": queries}))
PYTHON
```

From the repository root, query the stored envelopes and inspect native gRPC
trailers. Confirm the response includes the generated group messages. A
successful empty query alone does not prove message readback:

```sh
grpcurl -vv -import-path proto -proto backend/v1/backend.proto \
    -d @ xmtp.example.com:ASSIGNED_PORT \
    xmtp.backend.v1.QueryService/Query < query.json
```

Check an empty newest query too:

```sh
grpcurl -vv -import-path proto -proto backend/v1/backend.proto \
    -d '{}' xmtp.example.com:ASSIGNED_PORT \
    xmtp.backend.v1.QueryService/QueryNewest
```

Keep a subscription open for more than 15 minutes:

```sh
grpcurl -vv -max-time 1000 -import-path proto -proto backend/v1/backend.proto \
    -d @ xmtp.example.com:ASSIGNED_PORT \
    xmtp.backend.v1.SubscriptionService/SubscribeStatic < subscription.json
```

Confirm its Started frame arrives immediately. After 16 minutes, generate one
more message from a second terminal with the same `XDBG_DB_ROOT` and endpoint.
Confirm it arrives on the original response before the 1000 s client deadline.
The final `DeadlineExceeded` is expected from that client deadline. An earlier
disconnect is a failure. A reconnect does not prove that the original stream
survived. Run the remaining [ingress checks](/deploy/overview/#ingress-contract) too.

Inspect backend **Public Networking** and confirm it has no domain or TCP proxy.
Confirm that the HAProxy TCP proxy targets only `18443`. From outside Railway,
check that port 9464 is unreachable. Record the project, endpoint, commands, and
outputs before cleanup.

## Scaling and shutdown

Railway balances TCP connections randomly across replicas in the closest region.
It documents no affinity. gRPC multiplexes many streams over one long-lived
connection, so a client stays on one replica for that connection's life.
More than one replica can spread load unevenly. A Railway volume also limits
HAProxy scaling; keep this evaluation service at one replica.

Set `RAILWAY_DEPLOYMENT_DRAINING_SECONDS=30` on the backend to cover its drain
and telemetry flush budgets. See [deployment teardown](https://docs.railway.com/deployments/deployment-teardown). Test reconnects during a redeploy. Do not delete
the certificate volume to deploy a new HAProxy image.

## Verification record

Local checks on 2026-09-14 passed with `check-jsonschema 0.38.0` and Nix
HAProxy **3.4.3**. The embedded HAProxy configuration matches `dev/tls/haproxy.cfg`
exactly. The container built with HAProxy **3.2.23** and lego **4.35.2**, and its
configuration parsed too. The parse checks used a throwaway local certificate
only; they do not validate public certificate trust. xdbg built from this
checkout. Both `just lint-markdown` and `just docs lint` passed.

**Docs build stalled:** `npx astro build` stopped producing output at
`[content] Syncing content` in the sandbox. It was interrupted after several
minutes. A completed site build remains unverified.

**Live Railway checks are pending.** CLI 4.58.0 could not refresh authentication
in the verification sandbox: `Operation not permitted (os error 1)`, followed
by `Unauthorized`. No project or public endpoint has been created in this run.

**UNTESTED on Railway:** trusted DNS-01 issuance, certificate renewal and reload,
certificate persistence after redeploy, health, identity registration, group
creation, message publish and readback, native gRPC trailers, gRPC-Web, CORS,
backend exposure, external port 9464 reachability, and a subscription beyond
15 minutes. Task 7's local ingress results do not prove these Railway behaviors.
