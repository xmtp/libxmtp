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
have no user info, query, fragment, trailing slash, or dot path segment. Use
HTTP only for a local target. The target must serve objects at
`{base_url}/{hex SHA-256 digest}`. The endpoint must support path-style S3
requests. Keep the endpoint, bucket, and credential values private. The
backend validates the URL, upload limit, and retention limit at startup.

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

`retention_seconds = 0` means the backend promises no expiry. For a positive
value, set a target lifecycle rule that removes objects no earlier than that
period after upload. Check the target's lifecycle timing and clock behavior.
The backend publishes the retention value; it does not delete objects.

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
attachment bucket. Give the public GET path a separate read policy. If a
credential refresh fails, CreateUpload returns `UNAVAILABLE`. The backend logs
only the provider error kind. The response does not contain the credential or
the provider error text.
