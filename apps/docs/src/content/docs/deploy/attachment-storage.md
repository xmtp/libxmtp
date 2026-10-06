---
title: Attachment storage
description: Configure an S3-compatible target for encrypted attachments.
---

Attachment storage is optional. When you configure it, the backend publishes
`base_url`, `max_upload_bytes`, and `retention_seconds` to clients. The backend
signs one PUT request for each upload. Clients send the ciphertext to the
storage target and fetch it from `base_url`. The backend does not store the
ciphertext.

## Configure the target

```toml
[attachments]
base_url = "https://files.example.com/attachments"
max_upload_bytes = 10485760
retention_seconds = 2592000

[attachments.target.S3]
endpoint = "https://s3.example.com"
region = "us-east-1"
bucket = "attachments"
key_prefix = ""
presign_ttl_seconds = 900

[attachments.target.S3.credentials]
kind = "environment"
```

The public `base_url` must be an absolute HTTPS URL in production. It must
have no user info, query, fragment, trailing slash, or dot path segment. HTTP is accepted only for a loopback host, such as `127.0.0.1`,
`[::1]`, or `localhost`. A private LAN address still needs HTTPS. The target must serve objects at
`{base_url}/{hex SHA-256 digest}`. This backend signs path-style S3 requests,
so use an endpoint that supports them. Keep the endpoint, bucket, and
credential values private. The backend validates the URL, upload limit, and
retention limit at startup.

The target must enforce `If-None-Match: *` on PUT, return `412` when an object
exists, check `x-amz-checksum-sha256` against the request body, and reject a
request whose signed content length differs from the body length. A successful
GET from the public URL must return the exact bytes. Check these operations
before you use a new target in production.

Set CORS on the bucket when browsers upload attachments. Permit PUT and GET
from the application origins. Permit the `content-length`, `host`,
`if-none-match`, and `x-amz-checksum-sha256` request headers. Expose response
headers that the application needs. Use a specific origin list for a production
application.

The backend publishes the configured `retention_seconds`, or `0` when none is
configured. A published `0` is the no-expiry value in the offer; it does not
control the target. For positive retention, set the target's expiry rule to the
same period after upload. When the target keeps objects without expiry,
set `retention_seconds = 0` or omit it. Check the target's lifecycle timing and clock behavior.
The target applies its expiry rule; the backend does not delete objects.

`max_upload_bytes` defaults to `104857600` (100 MiB) and must be from `1`
through `4294967295`. `presign_ttl_seconds` defaults to `900` and must be from
`300` through `3600`. Temporary credentials can shorten the signed request's
lifetime. Credentials that leave less than 300 seconds cannot sign an upload.

## Credential kinds

The `credentials.kind` value selects how the backend gets signing credentials:

| Kind            | Required keys                                                             | Use                                         |
| --------------- | ------------------------------------------------------------------------- | ------------------------------------------- |
| `static`        | `access_key_id`, `secret_access_key`; optional `session_token`            | Local tests or an external secret injector. |
| `default_chain` | None                                                                      | AWS default provider chain.                 |
| `environment`   | None                                                                      | AWS credential environment variables.       |
| `profile`       | `name`                                                                    | Named AWS profile.                          |
| `sso`           | `account_id`, `region`, `role_name`, `start_url`; optional `session_name` | AWS IAM Identity Center.                    |
| `process`       | `command`                                                                 | External credential process.                |
| `web_identity`  | None                                                                      | Web identity token.                         |
| `container`     | None                                                                      | Container credential endpoint.              |
| `instance`      | None                                                                      | Instance metadata.                          |
| `assume_role`   | `role_arn`; optional `external_id`, `session_name`                        | AWS role assumption.                        |

Use `env:NAME` for secret settings in TOML. Limit the credential to PUT on the
attachment bucket. Give the public GET path a separate read policy. If the
backend cannot obtain credentials, or they expire too soon, CreateUpload returns
`UNAVAILABLE`. If the credential source reports an invalid configuration, it
returns `FAILED_PRECONDITION`. If the backend cannot build or sign the request,
it returns `INTERNAL`. The backend refuses to start if `credentials.kind` is
unknown or a required field is missing. `GetConfigurationResponse` contains no storage
credential, bucket name, or storage endpoint URL other than `base_url`.
