---
title: Run a local XMTP node with Tailscale
description: Run an XMTP backend on Linux or Mac and connect remotely through a private Tailscale HTTPS URL.
---

Run an XMTP self-hosted node on a Linux computer or Mac. Connect to it from another computer or phone with Tailscale. The node stays private to your Tailscale network, called a tailnet. Native SDKs and the Browser SDK use the same HTTPS URL.

This setup runs two containers: the XMTP backend and PostgreSQL. Tailscale runs on the host. You do not need to clone this repository, build the backend, register a domain, or forward a router port.

## Before you start

You need a host that stays powered on and connected to the internet, a Tailscale account, and a remote device. Install these tools on the host:

- **Linux:** [Docker Engine](https://docs.docker.com/engine/install/) and the [Docker Compose plugin](https://docs.docker.com/compose/install/linux/).

- **Mac:** [Docker Desktop](https://docs.docker.com/desktop/setup/install/mac-install/). Start Docker Desktop before you run the commands below. Both Intel and Apple Silicon Macs can run the backend image.

- **Both:** Current stable Tailscale. This guide uses the HTTPS reverse proxy in Tailscale Serve. Its native gRPC support is present in [Tailscale v1.104.1](https://github.com/tailscale/tailscale/releases/tag/v1.104.1).

Check Docker and Compose:

```sh
docker version
docker compose version
```

On Linux, use `sudo` for Docker commands if your installation requires it. The password command below also needs `openssl`.

## Step 1 Join the same tailnet

Install Tailscale on the host and each remote device. Sign in to the same tailnet on all devices.

On Linux, follow the [Tailscale installation guide](https://tailscale.com/docs/install/linux), then sign in:

```sh
sudo tailscale up
```

On Mac, install the [standalone Tailscale app](https://tailscale.com/docs/install/mac), open it, and sign in. In the app settings, open [**CLI integration**](https://tailscale.com/docs/reference/tailscale-cli?tab=macos), select **Show me how**, then **Install Now**. This step requires macOS 13 or later and an administrator password. It makes the `tailscale` command available in your terminal. The app can share local ports with Serve; you do not need the separate open source daemon.

If you already installed the command-line package with Homebrew, use its service instead:

```sh
brew upgrade tailscale
sudo brew services start tailscale
sudo tailscale up
```

Enter your Mac password when prompted. Open the login URL from `tailscale up` and sign in. See [the Homebrew package instructions](https://formulae.brew.sh/formula/tailscale).

Check the host connection:

```sh
tailscale version
tailscale status
```

Keep Tailscale running on the host and remote devices. On a shared tailnet, check who can reach the host. Tailscale [access rules](https://tailscale.com/docs/features/access-control/grants) apply to Serve. This setup relies on those rules for caller access. It does not enable backend API key authentication.

## Step 2 Create the node configuration

Create a folder outside the repository. Run the remaining Docker commands from this folder:

```sh
mkdir xmtp-local
cd xmtp-local
umask 077
printf 'POSTGRES_PASSWORD=%s\n' "$(openssl rand -hex 32)" > .env
```

Run the password command only when you create the database. Keep `.env`; changing its password later does not change the password in an existing PostgreSQL volume.

Create `config.toml`:

```toml
[server]
identifier = "org.example.xmtp"
listen = "0.0.0.0:5050"

[database]
url = "env:XMTP_DATABASE_URL"
```

Replace `org.example.xmtp` with a unique, stable name for your deployment. Keep that identifier after clients connect, including when you move the node. A client database is bound to the deployment identifier. See [backend configuration](/get-started/run-the-backend/#configuration).

Create `compose.yaml`:

```yaml
name: xmtp-local

services:
  db:
    image: postgres:17
    restart: unless-stopped
    environment:
      POSTGRES_USER: xmtp
      POSTGRES_DB: xmtp
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:?Set POSTGRES_PASSWORD in .env}
    volumes:
      - postgres-data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -h 127.0.0.1 -U xmtp -d xmtp"]
      interval: 5s
      timeout: 5s
      retries: 20

  backend:
    image: ghcr.io/xmtp/backend:8.0.0-rc1
    restart: unless-stopped
    stop_grace_period: 30s
    depends_on:
      db:
        condition: service_healthy
    environment:
      XMTP_DATABASE_URL: postgres://xmtp:${POSTGRES_PASSWORD:?Set POSTGRES_PASSWORD in .env}@db:5432/xmtp
    volumes:
      - ./config.toml:/etc/xmtp/config.toml:ro
    command: ["--config-file", "/etc/xmtp/config.toml"]
    ports:
      - "127.0.0.1:5050:5050"
    healthcheck:
      test: ["CMD", "/bin/grpc-health-probe", "-addr=127.0.0.1:5050"]
      interval: 5s
      timeout: 5s
      retries: 20

volumes:
  postgres-data:
```

Port `5050` is published only on host loopback. PostgreSQL and the metrics listener have no published host ports. The backend listens on all interfaces inside its container so Docker can forward traffic to it.

The named volume keeps PostgreSQL data when containers stop or are replaced. Keep the Compose project name stable so it selects the same volume. Back up PostgreSQL before an upgrade; backend schema changes can require a new database. See the [deployment requirements](/deploy/overview/).

## Step 3 Start the node

```sh
docker compose up -d --wait
docker compose ps
docker compose exec backend /bin/grpc-health-probe -addr=127.0.0.1:5050
```

Wait for both services to become healthy. The probe must report `SERVING`. The backend applies database migrations before it accepts connections.

If startup fails, read the logs:

```sh
docker compose logs --tail=100 db backend
```

## Step 4 Share the API with Tailscale Serve

On the host, run:

```sh
tailscale serve --bg --https=443 http://127.0.0.1:5050
tailscale serve status
```

On Linux, prefix Tailscale configuration commands with `sudo` if Tailscale reports a permission error. This also applies to the stop command below.

If Serve prints a setup link, open it and enable the required HTTPS settings. A tailnet administrator must approve this step. Tailscale uses MagicDNS and a certificate for the host's full `*.ts.net` name. You can also enable MagicDNS and HTTPS in the [DNS settings](https://console.tailscale.com/admin/dns).

HTTPS certificate names appear in public certificate transparency logs. Use a host name that you can make public. The certificate does not make the node reachable from the public internet. See [Tailscale HTTPS certificates](https://tailscale.com/docs/how-to/set-up-https-certificates).

Serve prints a URL such as:

```text
https://xmtp-node.tail1234.ts.net
```

Copy the URL from your own output. Tailscale supplies and renews the certificate. `--bg` keeps Serve active after the terminal closes and restores it when Tailscale restarts. See the [Serve command reference](https://tailscale.com/docs/reference/tailscale-cli/serve).

Use the HTTPS reverse proxy shown above. It supports both native gRPC and browser gRPC-Web on the same URL.

Use Serve for this private node. Tailscale Funnel publishes a service to the internet. You do not need Funnel, a subnet router, or an exit node for this setup.

## Step 5 Connect from a remote device

Connect the remote device to Tailscale. Test from a different network, such as a phone hotspot. In the SDK [client setup](/sdk/client/#create-a-client), set the backend URL to the full HTTPS URL that Serve printed. Use that URL for Browser, Node, Agent, Kotlin, and Swift clients.

Use the full `*.ts.net` name. A short host name or a `100.x.y.z` address does not match the HTTPS certificate. `127.0.0.1` on the remote device refers to that device, not to the node.

If you have [grpc-health-probe](https://github.com/grpc-ecosystem/grpc-health-probe) installed on the remote computer, check the complete native gRPC path:

```sh
xmtp_host=xmtp-node.tail1234.ts.net
grpc_health_probe -addr="$(tailscale ip -4 "$xmtp_host"):443" -tls \
  -tls-server-name="$xmtp_host" -connect-timeout=15s -rpc-timeout=10s
```

Set `xmtp_host` to the host name from your Serve URL, without `https://`. Expect `SERVING`. The probe connects to the Tailscale IP and verifies the certificate against the full host name. This also works when the probe's DNS resolver does not use MagicDNS. Keep certificate verification enabled.

The probe allows 15 seconds to connect and 10 seconds for the health request.

Complete the [SDK quickstart](/get-started/quickstart/) with two clients that use this same backend. Send a message and check that it arrives while the recipient's stream remains open. A health check alone does not test browser gRPC-Web or message subscriptions. The API URL does not serve a web page, and the backend has no HTTP `/health` endpoint.

## Keep the node available

Keep the host awake, Docker running, and Tailscale connected. On Mac, enable Docker Desktop startup at sign in. The container restart policy takes effect when the Docker engine starts; it cannot wake the host or start Docker Desktop before sign in.

Check the host's Tailscale device key expiry if it must run unattended. Use the [server setup guidance](https://tailscale.com/docs/how-to/set-up-servers) to choose a renewal or expiry policy. Disabling key expiry is optional and changes how long the device remains trusted.

On a shared tailnet, limit access to the node's TCP port `443` to the intended users or devices. Access grants are additive. A new narrow grant does not override an existing broad allow rule. Add [backend API key or JWT authentication](/get-started/run-the-backend/#auth) if network access alone is not enough.

Text messaging needs only the two containers above. [Attachment storage](/deploy/attachment-storage/) and [push delivery](/deploy/push-configuration/) need separate configuration. Remote clients must be able to reach attachment storage URLs. Native attachment downloads from private Tailscale addresses also need the client's `allowPrivateNetwork` option. See [attachment transfer rules](/content-types/attachments/#native-attachment-uploads-and-downloads). Smart contract wallet identities also need the chain RPC settings described in [backend configuration](/get-started/run-the-backend/#chains).

## Stop or resume the node

To stop the containers and remove this HTTPS listener:

```sh
tailscale serve --bg --https=443 off
docker compose down
```

The database volume remains. Do not add `--volumes` unless you intend to delete its data. `tailscale serve reset` removes all Serve configuration on the host, so use the port-specific command above when other services use Serve.

To resume, run `docker compose up -d --wait`, then run the Serve command from Step 4 again.

## Troubleshooting

| Symptom                                | Check                                                                                                                                                       |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Containers are unhealthy               | Read `docker compose logs --tail=100 db backend`. Check `.env`, database access, and the deployment identifier.                                             |
| Serve cannot reach the backend         | Check that `docker compose ps` shows a healthy backend with `127.0.0.1:5050` published. Tailscale must run on the host, not inside an unrelated container.  |
| Remote connection times out            | Check that both devices are connected to the same tailnet and the host is awake. Check access rules for TCP `443`.                                          |
| DNS or certificate error               | Use the exact full HTTPS name from `tailscale serve status`. Check MagicDNS and HTTPS settings.                                                             |
| Browser connects but native gRPC fails | Update the host's Tailscale to the current stable release. Check that Serve uses HTTPS reverse proxy mode with the explicit `http://127.0.0.1:5050` target. |
| Browser request fails                  | Check that Tailscale is connected on the device running the browser. Use HTTPS and confirm that browser preflight requests reach the backend.               |
| Messages do not arrive                 | Use the same backend for both clients. Check an open recipient stream, rather than only the health probe.                                                   |
| Client reports a backend mismatch      | Restore the deployment's original `server.identifier`. Do not change it when you change the host or URL.                                                    |

## Why this setup uses Serve

The HTTPS reverse proxy in Tailscale v1.104.1 advertises HTTP/2 and HTTP/1.1 during TLS negotiation. It forwards native gRPC requests to the local backend over plaintext HTTP/2, called h2c. It passes browser gRPC-Web requests to the same backend port. This lets the two client types share one HTTPS URL. See [the TLS protocol list](https://github.com/tailscale/tailscale/blob/v1.104.1/ipn/ipnlocal/cert.go), [the gRPC proxy implementation](https://github.com/tailscale/tailscale/blob/v1.104.1/ipn/ipnlocal/serve.go), and [the protocol tests](https://github.com/tailscale/tailscale/blob/v1.104.1/ipn/ipnlocal/serve_test.go). The TCP mode with TLS termination does not advertise HTTP/2, so it is not the ingress used here.

Other projects document similar private access setups:

- [Open WebUI Computer](https://docs.openwebui.com/ecosystem/computer/phone-and-remote/tailscale/) uses Serve for a service on localhost.

- [Nextcloud AIO](https://github.com/nextcloud/all-in-one/blob/main/reverse-proxy.md#secure-tunnel-using-aio-with-a-secure-tunneling-service-tailscale-cloudflare-pangolin) describes private access with Serve and loopback port binding.

- [Jellyfin](https://jellyfin.org/docs/general/post-install/networking/tailscale/) connects server and client devices through the same tailnet.

- [Immich](https://docs.immich.app/guides/remote-access/) describes Tailscale access when router port forwarding is unavailable.

- [Home Assistant Tailscale app](https://github.com/hassio-addons/app-tailscale/blob/main/tailscale/DOCS.md#option-share_homeassistant) describes private HTTPS with Serve and automatic certificate renewal.
