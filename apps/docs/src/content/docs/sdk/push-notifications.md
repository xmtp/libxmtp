---
title: Push notifications
---

The XMTP backend sends push notifications through APNs, FCM, or an HTTPS webhook.
The Android, iOS, and Node SDKs register the installation and keep subscriptions
current. The Browser SDK and WASM binding do not expose this API.

Ask the backend operator which channels are configured. See
[Push configuration](/get-started/push-configuration/) for server setup.
The example apps do not include push registration or a notification receiver.

## Enable and control notifications

These five methods are asynchronous. Failures throw an error or reject a promise.
Conversation methods are also available on `Group` and `Dm`.

| Method                                 | Result                                                             |
| -------------------------------------- | ------------------------------------------------------------------ |
| `client.enableNotifications(config)`   | Store the config, register the installation, and return its state. |
| `client.disableNotifications()`        | Disable locally, then unregister from the backend.                 |
| `client.notificationState()`           | Read local state without a backend request.                        |
| `conversation.setNotifications(value)` | Set an override, or reset it to the config rules.                  |
| `conversation.notificationsEnabled()`  | Read the effective value for the conversation.                     |

### Kotlin

```kotlin
suspend fun configurePush(client: Client, conversation: Conversation, token: String) {
    client.enableNotifications(NotificationConfig(NotificationChannel.Fcm(token)))
    conversation.setNotifications(NotificationOverride.Disabled)
    val enabled = conversation.notificationsEnabled()
    conversation.setNotifications(NotificationOverride.Default)
    val state = client.notificationState()
    if (state is NotificationState.Failed) {
        println(state.error.code)
    }
    client.disableNotifications()
}
```

### Swift

```swift
func configurePush(client: Client, conversation: Conversation, token: String) async throws {
    _ = try await client.enableNotifications(NotificationConfig(channel: .apns(token: token)))
    try await conversation.setNotifications(.disabled)
    let enabled = try await conversation.notificationsEnabled()
    try await conversation.setNotifications(.default)
    if case let .failed(error) = try await client.notificationState() {
        print(error.code)
    }
    try await client.disableNotifications()
}
```

### Node

```ts source="push-notifications-node.ts" region="configure"

```

These examples show each operation. In an app, call `disableNotifications` only
when the user turns notifications off. Call `enableNotifications` again when the
provider token, webhook URL, or rules change. A running client resumes stored
notification work when it opens the same local database.

## Configuration and defaults

`NotificationConfig.channel` is required. The variants are `Apns`, `Fcm`, and
`Http` on Kotlin; `.apns`, `.fcm`, and `.http` on Swift; and objects with `type`
equal to `"apns"`, `"fcm"`, or `"http"` on Node.

APNs and FCM need a `token`. HTTP needs an HTTPS `url` and a random 32-byte
`signingKey`. The receiver must keep the same signing key to verify requests.
Bytes use `ByteArray`, `Data`, and `Uint8Array`, respectively.

```ts source="push-notifications-node.ts" region="webhook"

```

| Field               | Default      | Meaning                                                                                         |
| ------------------- | ------------ | ----------------------------------------------------------------------------------------------- |
| `consentStates`     | Allowed only | Subscribe to active conversations with one of these consent states. An empty list selects none. |
| `includeWelcomes`   | `true`       | Subscribe to this installation's welcome topic.                                                 |
| `includeSyncGroups` | `false`      | Include device-sync groups.                                                                     |
| `includeCommits`    | `false`      | Include commits and proposals on subscribed group topics.                                       |
| `metadata`          | Empty bytes  | Up to 4096 bytes of recipient data. Returned only in webhook payloads.                          |

An enabled or disabled override takes priority over `consentStates`. Reset with
`NotificationOverride.Default`, `.default`, or `"default"`. Overrides cannot
enable a group after this installation leaves it. Device-sync groups follow
`includeSyncGroups` and have no per-conversation override.

The SDK updates subscriptions after consent, membership, and key changes. It
uploads sender-filter keys and renews registrations automatically while its task
runner is active. An update is asynchronous; it does not make messaging wait for
push registration. A lost wake can delay an update until the next hourly sync.
Sender filtering can miss during key rotation or an epoch boundary, so an app
must still suppress its own messages when it displays notifications.

## State and errors

Local state is `Disabled`, `Enabled`, or `Failed(error)` on Kotlin; `.disabled`,
`.enabled`, or `.failed(error)` on Swift; and a union with `state` set to
`"disabled"`, `"enabled"`, or `"failed"` on Node. Failed state contains a
`NotificationError` with a stable `code`.

Terminal codes have the prefix `NotificationError::`: `PermissionDenied`,
`InvalidArgument`, `OutOfRange`, `Unimplemented`, or `ChannelNotConfigured`.
They stop notification work. Correct the cause and call `enableNotifications`
again. `TaskRunnerDisabled` rejects enable without storing a config. Node apps
must not disable `WorkerKind.TaskRunner` if they use notifications.

Other registration failures leave the local state enabled so the task can retry.
`ResourceExhausted` waits for a change to the desired subscriptions before it
retries additions. Each notification request has a 30-second limit.

Disable keeps the local recipient identity and conversation overrides. It stays
disabled even if unregister fails. In that case the backend registration remains
until expiry. A later enable reuses the identity from the same database.

## Payload

Every push identifies an envelope. It contains no encrypted message, message
content, inbox ID, or recipient secret.

```json
{
  "topic": "AAECAwQFBgcICQoLDA0ODxA=",
  "sequence_id": "9007199254740993"
}
```

`topic` is standard base64 of the backend wire topic. Decode it before routing.
A group topic starts with `0x00` and contains a 16-byte group ID. A welcome topic
starts with `0x01` and contains a 32-byte installation key. The group ID is the
hex encoding of the bytes after the kind byte. Do not pass the base64 text to a
legacy string-topic lookup.

`sequence_id` is decimal text. Keep it as text or an exact integer such as
JavaScript `bigint`. Do not convert it to a JavaScript `number` or use it alone
as a message-delivery cursor.

APNs adds `"aps": { "content-available": 1 }`. FCM puts `topic` and
`sequence_id` in the message's `data` object. HTTPS adds `recipient_id` as hex
and `metadata` as base64. HTTPS requests carry the Standard Webhooks headers
`webhook-id`, `webhook-timestamp`, and `webhook-signature`. Verify the signature
over the exact body bytes before parsing, check the timestamp, and reject
replayed webhook IDs.

## Receive, fetch, and decrypt

The app owns notification reception and display. It must obtain the provider
token, request the required OS permissions, and install its background handler.
APNs sends a background notification, not an alert with display text. FCM sends
a data message. A notification does not contain the bytes accepted by
`processMessage` or `fromWelcome`.

1. Open the correct XMTP installation and its local database in the handler.
2. Validate the payload and decode the topic. A webhook receiver first verifies
   the signature. Treat a push as a sync hint, not as proof of a message.
3. Fetch and process pending welcomes. For a group topic, find the group by its
   decoded ID, then sync it to fetch and decrypt the envelopes. On mobile,
   `client.catchUpToLive(timeoutMs: ...)` provides a bounded sync; use the
   language's argument syntax. On Node, use the calls below.
4. Read decoded messages from the local database. Check consent, sender, and
   the app's display policy. Suppress messages already displayed, including
   notifications that repeat after a backend restart.
5. Show the permitted notification and complete the OS background callback.
   If the OS budget ends or the envelope is not available yet, keep work for
   the next allowed background run or foreground sync.

```ts source="push-notifications-node.ts" region="receive"

```

The snippet returns local messages for the app to filter. It does not implement
signature verification, display deduplication, OS scheduling, or user-interface
updates. A welcome can reveal an existing DM, so it need not produce an alert.
Push delivery can be delayed, dropped, or repeated. Normal message sync remains
the source of message state.

## DM stitching

Different installations can create separate groups for the same DM. The SDK
presents these groups as one visible DM and automatically subscribes to every
matching group. A DM lookup can return a different group ID as the underlying
groups converge. Use the peer inbox ID for app state that belongs to the DM.
Do not show a new-conversation alert when a welcome only adds a duplicate DM.

## Upgrade from the old push client

This is a breaking change. `XMTPPush`, the generated push-service stubs,
`getPushTopics`, and `allPushTopics` are removed. Apps must call
`enableNotifications` to register again. Previous subscriptions and
per-conversation notification choices are not migrated. The new default selects
only allowed conversations.

Rewrite receivers for the payload above. The old `encryptedMessage` field and
string routing topics are absent. The backend operator must decommission the
old notification server and remove its registrations. The new
`disableNotifications` cannot remove registrations from that server; old pushes
can continue until the operator completes cleanup.
