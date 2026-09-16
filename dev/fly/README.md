# Persistent development environment

This environment is separate from the disposable iOS test apps. All Fly apps
and the Managed Postgres cluster use organization `xmtp-labs` and region `sjc`.
Never include these names in the iOS cleanup scripts.

| Service | App or cluster | Access |
| --- | --- | --- |
| Backend | `xmtp-backend-dev` | `https://backend-dev.xmtp.to` |
| PostgreSQL 17 | `xmtp-backend-dev-db` | Private direct connection |
| Tempo | `xmtp-backend-dev-tempo` | Private OTLP and query listeners |
| Trace metrics | `xmtp-backend-dev-prometheus` | Private Tempo remote-write receiver |
| Grafana | `xmtp-backend-dev-grafana` | Local Fly proxy and Grafana login |
| Docs | GitHub Pages for `xmtp/libxmtp` | `https://self-hosted-docs.xmtp.to` |

The backend identifier is `org.xmtp.backend.dev`. Do not change it after clients
connect. The database and observability volumes persist across deployments.

## API key

From the repository root, generate a 256-bit random key:

```sh
dev/nix-shell 'cargo run -p xdbg -- generate-api-key'
```

Store the output in repository Actions secret `XMTP_BACKEND_DEV_API_KEY`.
The deployment workflow uses the existing `FLY_API_TOKEN` and imports the API
key into Fly through standard input. Keys do not belong in TOML files or logs.
For rotation, replace the GitHub secret and deploy again. Existing callers
must receive the new key before they can reconnect.

## Database

The cluster is Managed Postgres Basic, PostgreSQL 17, with 10 GB initial storage.
Its ID is `w86750819jjr3pk4`. Use the direct connection URL from the cluster's
Connect page. The hostname starts with `direct.`, not `pgbouncer.`. Migrations
need session state. Store this URL once as the backend's `XMTP_DATABASE_URL`:

```sh
fly secrets import --stage --app xmtp-backend-dev
```

Enter `XMTP_DATABASE_URL=` followed by the direct URL, then press Ctrl-D.
The regular deploy workflow preserves this secret and never recreates the
database. Keep one backend Machine until its connection budget is reviewed.

## Private Grafana access

Log in to Fly with an account that can access `xmtp-labs`, then run:

```sh
fly proxy 3000:3000 --bind-addr 127.0.0.1 --app xmtp-backend-dev-grafana
```

Keep the command running and open `http://127.0.0.1:3000`. Log in as `admin`
with the configured Grafana password. No public IP or HTTP service is configured
for Grafana, Tempo, or Prometheus. The proxy uses an authenticated WireGuard tunnel.

Run `bash dev/fly/setup-grafana-secrets` once before Grafana's first start.
It generates an admin password and a read-only Fly metrics token, then imports
them without printing their values. To read the initial password in your own
terminal, use `fly ssh console --app xmtp-backend-dev-grafana --command
'printenv GF_SECURITY_ADMIN_PASSWORD'`. Store it in your password manager.
Grafana stores the login in its volume after that first start;
later password changes must use Grafana's password reset command or UI.

Grafana's `FLY_METRICS_AUTHORIZATION` secret contains the complete authorization
header for a read-only token scoped to `xmtp-labs`. Create one with:

```sh
fly tokens create readonly --org xmtp-labs --name backend-dev-grafana --expiry 8760h
```

Tokens from this command already include the `FlyV1` prefix. Store that complete
value through `fly secrets import --app xmtp-backend-dev-grafana`; do not add a
second prefix. Rotate the token before its one-year expiry.

Clients can send test traces through a separate private tunnel:

```sh
fly proxy 4317:4317 --bind-addr 127.0.0.1 --app xmtp-backend-dev-tempo
```

Use `http://127.0.0.1:4317` as the client's OTLP endpoint. Client panels need
client traces; backend traces alone cannot populate them.

## Deployments

Every push to `self-hosted` publishes an immutable backend image, then calls
`deploy-backend-dev.yml`. That workflow serializes deploys and skips superseded
commits. It deploys observability configuration, then the backend. Missing API
keys fail deployment. There is no unauthenticated fallback.

For a manual deployment from the repository root:

```sh
bash dev/fly/deploy-observability
bash dev/fly/deploy-backend ghcr.io/xmtp/backend:sha-FULL_COMMIT
```

The second command requires `XMTP_BACKEND_DEV_API_KEY` in the environment.
The dashboard is generated from `dev/docker/grafana/dashboards/backend.json`.
Edit that source, not the generated file. Fly queries are restricted to this
backend app. Provisioned dashboard edits in the Grafana UI are disabled.

Tempo keeps three days of traces. Its local volume is suitable for this test
environment, but is not a highly available trace store. The backend, Tempo,
and Grafana each run on one Machine. Deployments can interrupt connections.
Fly metrics retain approximately 15 days of data.

Private Prometheus stores Tempo-generated span metrics and service graphs for
up to three days, with a 1 GB time-series storage limit on a 3 GB volume.
Backend metrics use Fly's managed metrics service. The generated dashboard
selects the correct data source for each panel. All private services are
reachable by other apps and authorized users on the organization's Fly network.

## Custom domains

Register the backend domain before adding its DNS records:

```sh
fly certs add backend-dev.xmtp.to --app xmtp-backend-dev
fly certs setup backend-dev.xmtp.to --app xmtp-backend-dev
```

In Cloudflare, create the CNAME for `backend-dev` with target
`o9en1wj.xmtp-backend-dev.fly.dev`. Set it to **DNS only**.
For certificate issuance before the app is ready, add another DNS-only CNAME:
`_acme-challenge.backend-dev` to `backend-dev.xmtp.to.o9en1wj.flydns.net`.
Add any other certificate verification records shown by
Fly. Check issuance with:

```sh
fly certs check backend-dev.xmtp.to --app xmtp-backend-dev
```

For the docs, set the repository's Pages custom domain to
`self-hosted-docs.xmtp.to` first. Then add a DNS-only Cloudflare CNAME named
`self-hosted-docs` with target `xmtp.github.io`. Enable **Enforce HTTPS** in
Pages settings after certificate issuance.

The existing docs workflow builds the site and Rust, Swift, Kotlin, and
JavaScript references on every push to `self-hosted`. It publishes only after
the composed site's checks pass. Pull requests validate without publishing.

## Checks and recovery

```sh
fly checks list --app xmtp-backend-dev
grpc-health-probe -addr=backend-dev.xmtp.to:443 -tls
```

Health and deployment configuration RPCs do not need authentication. Application
RPCs require the key as a bearer token. Verify both rejected and accepted calls
after a key change. Check the XMTP dashboard for metrics and traces after traffic.

Redeploy a previous immutable image to recover from a binary regression only
when its database schema is compatible. Do not assume binary rollback reverses
migrations. Use Managed Postgres backups to recover database state. Restore
into a separate cluster and validate it before changing `XMTP_DATABASE_URL`.

The current backend development rules allow edits to an existing migration.
Such an edit can fail checksum validation against this persistent database.
Resolve that with an explicit migration and data plan. Never reset this database
automatically to make a deployment pass.
