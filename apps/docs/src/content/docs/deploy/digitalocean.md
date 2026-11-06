---
title: Deploy on DigitalOcean
description: Host the XMTP backend on App Platform with Managed PostgreSQL and R2 attachments, using Terraform or manual setup.
---

Run the public backend image on DigitalOcean App Platform. App Platform
terminates HTTPS and forwards h2c to the backend. This guide uses one 2 GiB
app instance, a private 2 GiB PostgreSQL 17 database, and Cloudflare R2 for
attachments. Choose either Terraform or manual setup below.

Read the [deployment overview](/deploy/overview/) for migrations, backups,
shutdown, authentication, and the ingress contract. The single database node
has no standby failover. Check current [App Platform pricing](https://www.digitalocean.com/pricing/app-platform)
and [database pricing](https://www.digitalocean.com/pricing/managed-databases)
before creating resources.

## Prepare attachment storage

Both options use an existing R2 bucket. Create it in the Cloudflare dashboard:

1. Create a dedicated bucket, such as `xmtp-attachments`. Leave object expiry
   disabled; the backend will publish `retention_seconds = 0`.
2. Create an [R2 API token](https://developers.cloudflare.com/r2/api/tokens/)
   with **Object Read & Write**, scoped to this bucket. Save the **Access Key
   ID** and **Secret Access Key** shown at creation. These are S3 credentials,
   not the Cloudflare management API token.
3. Connect a [custom domain](https://developers.cloudflare.com/r2/buckets/public-buckets/#connect-a-bucket-to-a-custom-domain),
   such as `attachments.example.com`, in a zone you control. This enables
   public reads of encrypted objects. Keep the development `r2.dev` URL disabled;
   it is rate limited and is not intended for production.
4. For browser clients, configure [bucket CORS](https://developers.cloudflare.com/r2/buckets/cors/)
   for their application origins. Allow `GET`, `HEAD`, and `PUT`, with
   `content-type`, `content-length`, `host`, `if-none-match`, and
   `x-amz-checksum-sha256` request headers. Expose `ETag` and
   `x-amz-checksum-sha256` response headers.

Record the bucket name, the S3 endpoint
`https://<ACCOUNT_ID>.r2.cloudflarestorage.com`, and the download base URL
`https://attachments.example.com` without a trailing slash. Signed uploads go
to the S3 endpoint; downloads use the custom domain. R2 presigned URLs do
not use custom domains. See [attachment storage](/deploy/attachment-storage/)
for the required conditional PUT, checksum, content-length, and download checks.

## Option 1: Terraform

Download the complete [single-file Terraform setup](/examples/digitalocean/main.tf)
as `main.tf` in a new directory outside this repository. It creates the app,
a dedicated VPC, PostgreSQL, a database firewall, and an API key. The backend
TOML is embedded in the file. It uses the R2 bucket and keys prepared above;
it does not manage Cloudflare resources.

Install [Terraform](https://developer.hashicorp.com/terraform/install) 1.7 or
newer and create a [DigitalOcean API token](https://docs.digitalocean.com/reference/api/create-personal-access-token/)
that can manage Apps, Databases, and VPCs. The example pins DigitalOcean provider
2.106.x and REST provider 3.0.x. DigitalOcean's
[provider 2.106.0](https://github.com/digitalocean/terraform-provider-digitalocean/blob/v2.106.0/digitalocean/app/app_spec.go) does not expose
the service `protocol` field, so the app uses the REST provider to set
[`protocol = "HTTP2"`](https://docs.digitalocean.com/products/app-platform/how-to/configure-http2/).
The other resources use the official provider.

Save these non-secret inputs as `terraform.tfvars`, replacing the examples:

```hcl
name                 = "my-xmtp-backend"
server_identifier    = "org.example.xmtp"
backend_image        = "ghcr.io/xmtp/backend:8.0.0-rc4"
r2_endpoint          = "https://<ACCOUNT_ID>.r2.cloudflarestorage.com"
r2_bucket            = "xmtp-attachments"
attachments_base_url = "https://attachments.example.com"
```

Keep `server_identifier` unchanged after clients connect. Pin `backend_image`
to a release, a full `sha-<commit>` tag, or a digest; avoid the moving
`:self-hosted` tag. Public GHCR images need no registry credentials.

In Bash, read the three credentials without putting them in shell history:

```bash
read -r -s -p "DigitalOcean token: " TF_VAR_digitalocean_token
printf '\n'
read -r -s -p "R2 access key ID: " TF_VAR_r2_access_key_id
printf '\n'
read -r -s -p "R2 secret access key: " TF_VAR_r2_secret_access_key
printf '\n'
export TF_VAR_digitalocean_token TF_VAR_r2_access_key_id TF_VAR_r2_secret_access_key
export API_DATA_IS_SENSITIVE=true

terraform init
terraform validate
terraform plan -out=deployment.tfplan
terraform apply deployment.tfplan
```

`API_DATA_IS_SENSITIVE=true` makes the REST provider redact computed API
responses that contain secrets. Set it in your Terraform runner too, and
leave provider debug logging disabled. Terraform state and saved plans still
contain credentials. Keep them out of Git and use an access-controlled,
encrypted remote state backend for a shared deployment. Commit the generated
`.terraform.lock.hcl` in your infrastructure project.

Apply submits the app; it does not wait for a successful deployment. Use the
`app_id` output to watch it in the DigitalOcean console. Once it is active,
refresh and retrieve the URL and generated key:

```sh
terraform apply -refresh-only
terraform output -json backend_connection
```

This prints the secret API key. The output contains `domain`, `url`, and
`api_key`; the domain and URL can be null during initial deployment. Use the
key as `Bearer <api_key>` in the client credential callback. See
[backend authentication](/sdk/client/#backend-authentication).

Make app changes through Terraform. The REST resource preserves its desired
request because DigitalOcean returns wrapped responses and encrypted secret
values; it does not detect every out-of-band app-spec change. Do not also
manage this app with the manual option.

## Option 2: Manual setup

Install [doctl](https://docs.digitalocean.com/reference/doctl/how-to/install/)
and run `doctl auth init`. Use the DigitalOcean control panel for the network
and database, then submit the app spec below with `doctl`.

### Create the VPC and database

1. [Create a dedicated VPC](https://docs.digitalocean.com/products/networking/vpc/how-to/create/)
   in **San Francisco 3 (`sfo3`)**. Record its UUID and private CIDR.
2. [Create a Managed PostgreSQL cluster](https://docs.digitalocean.com/products/databases/postgresql/how-to/create/)
   in `sfo3`, on that VPC. Select PostgreSQL **17**, **1 vCPU / 2 GiB RAM**,
   **30 GiB storage**, and **one node**.
3. [Create a database](https://docs.digitalocean.com/products/databases/postgresql/how-to/manage-users-and-databases/)
   named `xmtp_backend`. Use the cluster's `doadmin` user for this setup so
   startup can run migrations.
4. In the database's [trusted sources](https://docs.digitalocean.com/products/databases/postgresql/how-to/secure/),
   allow the dedicated VPC's CIDR. Remove any broad public-network rule.
   This authorizes the whole dedicated VPC. For a narrower rule, replace it
   with the app's VPC egress private IP after deployment.
5. From [**Connection Details**](https://docs.digitalocean.com/products/databases/postgresql/how-to/connect/), select **Private network** and copy the
   direct connection URI for `xmtp_backend`, with `sslmode=require`.
   Do not use a transaction-mode connection pool. `sslmode=require` encrypts
   the connection; it does not verify the server certificate's identity.

The default pool of 20 plus one tailer uses 21 connections per instance.
Two instances during a replacement need 42, leaving little room in the
2 GiB database's 47-connection budget. Allow room for migrations and admin
sessions; increase database capacity before scaling the app or enabling push
delivery. See [DigitalOcean connection limits](https://docs.digitalocean.com/products/databases/postgresql/details/limits/).

### Create the app

Create a private working directory (`umask 077`) and save this as `app.yaml`.
Replace every placeholder in your editor, including the VPC UUID, identifier,
database URI, R2 values, and a new API key. Generate a 64-character key with
`openssl rand -hex 32` and store it in your password manager. The file contains
secrets; keep it out of Git and shared directories.

```yaml
name: my-xmtp-backend
region: sfo
vpc:
  id: REPLACE_WITH_VPC_UUID
ingress:
  rules:
    - match:
        path:
          prefix: /
      component:
        name: backend
        preserve_path_prefix: true
services:
  - name: backend
    image:
      registry_type: GHCR
      registry: xmtp
      repository: backend
      tag: 8.0.0-rc4
    http_port: 5050
    protocol: HTTP2
    instance_count: 1
    instance_size_slug: apps-s-1vcpu-2gb
    health_check:
      initial_delay_seconds: 120
    termination:
      drain_seconds: 15
      grace_period_seconds: 30
    envs:
      - key: XMTP_CONFIG
        scope: RUN_TIME
        type: GENERAL
        value: |
          [server]
          identifier = "org.example.xmtp"
          listen = "0.0.0.0:5050"
          log_format = "json"

          [database]
          url = "env:XMTP_DATABASE_URL"
          max_connections = 20

          [auth]
          enabled = true

          [auth.api_keys]
          app = "env:XMTP_BACKEND_API_KEY"

          [attachments]
          base_url = "env:XMTP_ATTACHMENTS_BASE_URL"
          retention_seconds = 0

          [attachments.target.S3]
          endpoint = "env:XMTP_R2_ENDPOINT"
          region = "auto"
          bucket = "env:XMTP_R2_BUCKET"
          key_prefix = ""

          [attachments.target.S3.credentials]
          kind = "environment"
      - key: XMTP_DATABASE_URL
        scope: RUN_TIME
        type: SECRET
        value: "REPLACE_WITH_PRIVATE_DATABASE_URI"
      - key: XMTP_BACKEND_API_KEY
        scope: RUN_TIME
        type: SECRET
        value: "REPLACE_WITH_GENERATED_API_KEY"
      - key: XMTP_ATTACHMENTS_BASE_URL
        scope: RUN_TIME
        type: GENERAL
        value: https://attachments.example.com
      - key: XMTP_R2_ENDPOINT
        scope: RUN_TIME
        type: SECRET
        value: https://REPLACE_WITH_ACCOUNT_ID.r2.cloudflarestorage.com
      - key: XMTP_R2_BUCKET
        scope: RUN_TIME
        type: SECRET
        value: xmtp-attachments
      - key: AWS_ACCESS_KEY_ID
        scope: RUN_TIME
        type: SECRET
        value: "REPLACE_WITH_R2_ACCESS_KEY_ID"
      - key: AWS_SECRET_ACCESS_KEY
        scope: RUN_TIME
        type: SECRET
        value: "REPLACE_WITH_R2_SECRET_ACCESS_KEY"
```

Submit the spec and wait for deployment:

```sh
doctl apps create --spec app.yaml --wait --wait-timeout 15m
```

Save the returned app ID. Later changes use
`doctl apps update "$app_id" --spec app.yaml --wait --wait-timeout 15m`.
Keep secrets in protected storage and remove the plaintext working file when
finished. You can also edit the deployed spec through **Settings → App Spec**.
See [app creation](https://docs.digitalocean.com/reference/doctl/reference/apps/create/)
and the [app spec reference](https://docs.digitalocean.com/products/app-platform/reference/app-spec/).

## Settings that matter

- **HTTP/2:** `protocol: HTTP2` selects h2c from App Platform to port 5050.
  Keep the `/` route and `preserve_path_prefix: true` so RPC paths survive.
  See [Configure HTTP/2](https://docs.digitalocean.com/products/app-platform/how-to/configure-http2/).
- **Health:** omit `http_path` to get a TCP check. HTTP checks use HTTP/1.1
  even on an HTTP/2 service, and the backend has no HTTP health path. The
  120-second startup delay allows migrations to finish. See
  [health checks](https://docs.digitalocean.com/products/app-platform/how-to/manage-health-checks/).
- **Private networking:** App Platform calls San Francisco `sfo`; the VPC
  and database use `sfo3`. Keep all three in the same mapped datacenter.
  Use the private database host. See [App Platform VPC support](https://docs.digitalocean.com/products/app-platform/how-to/enable-vpc/).
  Both examples use `sslmode=require`: database traffic is encrypted, but
  server certificate identity is not verified. Full verification requires
  making the cluster's CA certificate available to the backend.
- **Runtime config:** use inline TOML in `XMTP_CONFIG`, with `env:NAME`
  references for secrets. Do not also set `--config-file` or a custom run
  command. Keep metrics port 9464 private.

## Verify the deployment

Find the app's default HTTPS URL in the console, or with:

```sh
app_id=REPLACE_WITH_APP_ID
doctl apps get "$app_id" --format DefaultIngress --no-header
doctl apps logs "$app_id" backend --type run --tail 100
grpc-health-probe -addr=YOUR_APP.ondigitalocean.app:443 -tls
```

Use the host without `https://` in the health command. A successful health
probe needs no API key. It is stronger than the platform's TCP check, but it
does not prove the full client flow.

Run the [ingress checks](/deploy/overview/#ingress-contract) with both a native
SDK and a browser client: register two identities, send and receive a message,
keep a subscription open, reconnect after a deployment, and upload and download
an attachment. Check byte-exact downloads and rejected duplicate or corrupted
uploads. Successful unary calls alone do not prove streaming or trailers work.

If startup fails, check the private database URL, VPC and trusted-source rule,
PostgreSQL version, and migration logs. If native clients fail while other
requests work, check `protocol: HTTP2` and the ingress path before changing
credentials. Browser attachment failures often need bucket CORS changes,
separate from the backend's gRPC-Web CORS handling.
