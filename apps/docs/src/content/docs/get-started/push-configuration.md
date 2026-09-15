---
title: Push configuration
description: Enable APNs, FCM, or HTTPS notifications on a self-hosted backend.
---

The backend sends notifications itself. Configure each channel that apps can
use. A channel is available only when its block is present. Without a `[push]`
section, every registration returns `FAILED_PRECONDITION`. An empty `[push]`
section enables no channels.

Apps register through the [SDK notification API](/sdk/push-notifications/).
The app's provider token is separate from the backend's provider credentials.
Protect the client-to-backend connection with HTTPS because registration sends
the recipient secret through that connection.

## Choose channels

| Channel | Operator supplies                                          | App supplies                                            |
| ------- | ---------------------------------------------------------- | ------------------------------------------------------- |
| APNs    | One APNs key, team ID, key ID, bundle ID, and environment. | An APNs device token for that app and environment.      |
| FCM     | One Firebase service-account JSON document.                | An FCM registration token from that project.            |
| HTTPS   | An optional host allowlist.                                | An HTTPS receiver URL and a random 32-byte signing key. |

Configure only the channels used by your apps. Each backend supports one APNs
configuration and one Firebase project. The HTTPS channel can serve multiple
receivers. Provider secrets load at startup; restart each backend instance after
a change. Store secret contents in environment variables or your secret manager.

## Configuration

```toml
[push]
recipient_ttl_seconds = 2592000
max_attempts = 3

[push.apns]
key = "env:XMTP_APNS_KEY"
key_id = "ABC123DEFG"
team_id = "TEAM123456"
bundle_id = "org.example.app"
environment = "production"

[push.fcm]
service_account = "env:XMTP_FCM_SERVICE_ACCOUNT"

[push.http]
allowed_domains = ["hooks.example.com", "*.example.org"]
allow_private_addresses = false

[limits]
max_push_topics = 100000
```

| Setting                             | Default             | Constraint                                              |
| ----------------------------------- | ------------------- | ------------------------------------------------------- |
| `push.recipient_ttl_seconds`        | `2592000` (30 days) | At least `86400`. Registrations without renewal expire. |
| `push.max_attempts`                 | `3`                 | From `1` to `10` attempts per delivery.                 |
| `push.apns.environment`             | `production`        | `production` or `sandbox`. Must match the app token.    |
| `push.http.allowed_domains`         | Empty list          | Empty permits any public host.                          |
| `push.http.allow_private_addresses` | `false`             | Use `true` only for development or tests.               |
| `limits.max_push_topics`            | `100000`            | Maximum subscriptions per recipient.                    |

All APNs fields except `environment` are required when `[push.apns]` is present.
`service_account` is required when `[push.fcm]` is present. `key` contains a
PKCS#8 PEM key; `service_account` contains the full JSON document. `env:NAME`
reads the variable's contents, not a file path. These secrets do not appear in
backend logs, error messages, or debug output.

Use the operator procedures for
[APNs credentials](https://github.com/xmtp/libxmtp/blob/self-hosted/docs/self-hosted/backend-operations.md#apns-credentials)
and [FCM credentials](https://github.com/xmtp/libxmtp/blob/self-hosted/docs/self-hosted/backend-operations.md#fcm-credentials).
Apple delivery through FCM also needs the APNs key configured in Firebase.

The [backend JSON schema](https://github.com/xmtp/libxmtp/blob/self-hosted/docs/schemas/backend-v1.json)
describes every setting. Missing required provider fields, invalid values, or
invalid allowlist entries fail startup and name the setting.

## HTTPS restrictions

An empty `[push.http]` block enables HTTPS delivery to any public host. To limit
destinations, set `allowed_domains`. Entries contain host names only, optionally
with a leading `*.`. Schemes, ports, paths, empty labels, and other star positions
are invalid. Matching is case-insensitive.

`hooks.example.com` matches that exact host. `*.example.org` matches
`a.example.org` and `a.b.example.org`, but not `example.org`. The list is checked
at registration. A registered URL remains usable until the next registration,
even if the allowlist changes.

URLs must use HTTPS. By default, registration and each delivery reject hosts
that resolve to private, loopback, link-local, or unspecified addresses. This
includes IPv6 equivalents. The sender connects only to a checked address and
does not follow redirects. `allow_private_addresses = true` permits those
addresses for local tests; it does not permit plaintext HTTP.

Each webhook carries a Standard Webhooks signature. The receiver must verify it
against the exact body bytes with the signing key supplied by the app. See the
[payload and receive path](/sdk/push-notifications/#payload).

## Delivery and operations

One backend instance holds the dispatcher lock. Other instances take over when
that connection ends. Pushes contain routing data, not message content. Clients
fetch and decrypt messages after receipt. Delivery may repeat after a crash, so
receivers must suppress duplicates.

Each attempt has a 10-second deadline. APNs and HTTPS retries wait at least one
second. FCM quota retries wait at least 60 seconds; other FCM retry responses
honor `Retry-After`, with a maximum of 300 seconds. Attempts stop at
`push.max_attempts`. Provider acceptance does not prove that the device displayed
a notification.

Watch `xmtp_push_deliveries_total` by `channel` and `outcome`, and
`xmtp_push_dispatcher` for the active dispatcher. A `mismatch` can mean a wrong
APNs environment, bundle ID, or Firebase project. Correct the configuration;
mismatches keep registrations. See
[provider failure operations](https://github.com/xmtp/libxmtp/blob/self-hosted/docs/self-hosted/backend-operations.md#delivery-failures).

Removing a provider block stops new registrations for that channel. It does not
delete existing registrations. Dead recipients and recipients that stop renewing
are deleted by the backend. Expiry defaults to 30 days without renewal.

When replacing the old notification server, decommission it and remove its
registrations as a separate operator task. New SDK registration and disable
calls cannot clean up the old server. Tell app developers to register again,
reset their notification choices, and update their receivers for the new payload.
