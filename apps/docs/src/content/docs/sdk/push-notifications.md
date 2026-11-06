---
title: Push notifications
---

The XMTP backend sends push notifications through APNs, FCM, or an HTTPS webhook.
The Android, iOS, Node, and React Native SDKs register a recipient for the local database and keep subscriptions
current. The Browser SDK and WASM binding do not expose this API.

Ask the backend operator which channels are configured. See
[Push configuration](/deploy/push-configuration/) for server setup.
The Android example has an optional FCM build. The iOS example at `apps/example-ios`
has an optional `Push` configuration with APNs registration and a silent-push receiver.
Debug and Release keep push off. APNs delivery needs a signed device build and a backend
with the APNs channel configured; the local development backend does not configure APNs.

## Enable and control notifications

Enable, disable, override updates, and conversation state reads are asynchronous.
`notificationState()` is synchronous and reads local state. Failures throw an error
or reject a promise. Conversation methods are available on `Group` and `Dm`.

| Method                                 | Result                                                                             |
| -------------------------------------- | ---------------------------------------------------------------------------------- |
| `client.enableNotifications(config)`   | Store the config, register the recipient, and return its local state.              |
| `client.disableNotifications()`        | Disable locally, then unregister from the backend.                                 |
| `client.notificationState()`           | Read local state without a backend request.                                        |
| `conversation.setNotifications(value)` | Set an override, or reset it to the config rules.                                  |
| `conversation.state()`                 | Read `common.notificationsEnabled` for a group or `notificationsEnabled` for a DM. |

### Kotlin

```kotlin
import uniffi.xmtp_sdk.*

suspend fun configurePush(client: SDKClient, conversation: Conversation, token: String) {
    client.enableNotifications(NotificationConfig(NotificationChannel.Fcm(token)))
    conversation.setNotifications(NotificationOverride.Disabled)
    val enabled = when (conversation) {
        is Conversation.Group -> conversation.group.state().common.notificationsEnabled
        is Conversation.Dm -> conversation.dm.state().notificationsEnabled
    }
    println(enabled)
    conversation.setNotifications(NotificationOverride.Default)
    val state = client.notificationState()
    if (state is NotificationState.Failed) {
        println(state.error)
    }
    client.disableNotifications()
}
```

### Swift

```swift
import XmtpSdk

func configurePush(client: SDKClient, conversation: Conversation, token: String) async throws {
  try await client.enableNotifications(config: NotificationConfig(channel: .apns(token: token)))
  try await conversation.setNotifications(value: .disabled)
  let enabled: Bool
  switch conversation {
  case let .group(group):
    enabled = try await group.state().common.notificationsEnabled
  case let .dm(dm):
    enabled = try await dm.state().notificationsEnabled
  }
  print(enabled)
  try await conversation.setNotifications(value: .default)
  if case let .failed(error) = try client.notificationState() {
    print(error)
  }
  try await client.disableNotifications()
}
```

### React Native

The app requests OS permission and obtains an APNs or FCM token. It passes the
token to the SDK; it must call enable again when that token changes.

```ts source="push-notifications-react-native.ts" region="configure"

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
`Http` on Kotlin; `.apns`, `.fcm`, and `.http` on Swift; and objects with `kind`
equal to `"apns"`, `"fcm"`, or `"http"` on Node and React Native.

APNs and FCM need a `token`. HTTP needs an HTTPS `url` and a `signingKey` of 16 to 64 bytes.
Use a random 32-byte key. The receiver must keep the same signing key to verify requests.
Bytes use `ByteArray`, `Data`, and `Uint8Array`, respectively.

```ts source="push-notifications-node.ts" region="webhook"

```

| Field               | Default      | Meaning                                                                                         |
| ------------------- | ------------ | ----------------------------------------------------------------------------------------------- |
| `consentStates`     | Allowed only | Subscribe to active conversations with one of these consent states. An empty list selects none. |
| `includeWelcomes`   | `true`       | Subscribe to this installation's welcome topic.                                                 |
| `includeSyncGroups` | `false`      | Include device-sync groups.                                                                     |
| `includeCommits`    | `false`      | Include commits and proposals on subscribed group topics.                                       |

An enabled or disabled override takes priority over `consentStates`. Reset with
`NotificationOverride.Default`, `.default`, or `"default"`. Overrides cannot
enable a group after this installation leaves it. Device-sync groups follow
`includeSyncGroups` and have no per-conversation override. The value in
conversation state reports the local rule. It does not confirm backend
subscription delivery or active membership.

The SDK updates subscriptions after consent, membership, and key changes. It
uploads sender-filter keys and renews registrations automatically while its task
runner is active. An update is asynchronous; it does not make messaging wait for
push registration. A lost wake can delay an update until the next periodic sync, which normally runs
at least once an hour.
Sender filtering can miss during key rotation or an epoch boundary, so an app
must still suppress its own messages when it displays notifications.

## State and errors

Local state is `Disabled`, `Enabled`, or `Failed(error)` on Kotlin; `.disabled`,
`.enabled`, or `.failed(error)` on Swift; and a union with `kind` set to
`"disabled"`, `"enabled"`, or `"failed"` on Node and React Native. Failed state contains a
`NotificationFailure` enum value. Kotlin uses values such as
`CHANNEL_NOT_CONFIGURED`, Swift uses `.channelNotConfigured`, and Node and React Native use
`"channelNotConfigured"`.

Terminal failures are permission denied, invalid argument, out of range,
unimplemented, and channel not configured. Operation errors use the matching
public error variants and codes: `PermissionDenied`, `InvalidArgument`,
`OutOfRange`, `Unimplemented`, and `ChannelNotConfigured`.
These failures stop notification work. Correct the cause and call `enableNotifications`
again. `TaskRunnerDisabled` rejects enable without storing a config. Keep the task runner enabled if the app uses notifications. Configure it through
`ClientOptions.workers`; Node and React Native name this worker kind `"taskRunner"`.

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
  "topic": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8g",
  "sequence_id": "9007199254740993"
}
```

`topic` is standard base64 of the backend wire topic. Pass `topic` and
`sequence_id` to the SDK's `parsePushHint`. It rejects a malformed payload with
`InvalidArgument` and returns the topic kind and identifier: a
`groupMessages` topic carries the conversation ID, and a `welcomeMessages` topic
carries the installation ID. Push subscriptions carry only these two kinds, so
ignore any other. [Topics](/protocol/topics/) describes the byte layout. Do not
pass the base64 text to a legacy string-topic lookup.

`sequence_id` is decimal text. `parsePushHint` returns it as an exact integer,
a `bigint` in JavaScript. Do not convert it to a JavaScript `number` or use it
alone as a message-delivery cursor.

APNs adds `"aps": { "content-available": 1 }`. FCM puts `topic` and
`sequence_id` in the message's `data` object. HTTPS adds `recipient_id` as hex.
The HTTPS payload contains only `topic`, `sequence_id`, and `recipient_id`.
HTTPS requests carry the Standard Webhooks headers
`webhook-id`, `webhook-timestamp`, and `webhook-signature`. Verify the signature
over the exact body bytes before parsing, check the timestamp, and reject
replayed webhook IDs.

## Receive, fetch, and decrypt

The app owns notification reception and display. It must obtain the provider
token, request the required OS permissions, and install its background handler.
APNs sends a background notification, not an alert with display text. FCM sends
a data message. A notification does not contain encrypted envelope bytes.
Use the SDK sync methods to fetch and process the envelopes.

1. Open the correct XMTP installation and its local database in the handler.
2. Validate the payload with `parsePushHint`. A webhook receiver first verifies
   the signature. Treat a push as a sync hint, not as proof of a message.
3. Fetch and process pending welcomes. For a group topic, find the group by its
   conversation ID, then sync it to fetch and decrypt the envelopes. Kotlin uses
   `client.catchUpToLive(timeoutMs = 5_000uL)` for a bounded sync. Swift uses
   `try await client.catchUpToLive(timeoutMs: 5_000)`. Check the returned
   summary's `completed` and `failed` fields. Node also exposes
   `catchUpToLive`; the example below uses explicit welcome and conversation sync.
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

### React Native foreground or permitted background JavaScript

When an OS integration has started a live JavaScript runtime, use the same
sync APIs as other native clients. `parsePushHint` validates the payload
without a base64 decoder from your app, and returns the sequence as a `bigint`:

```ts source="push-notifications-react-native.ts" region="receive"

```

The app must arrange the native APNs or FCM handler before any foreground UI
mounts. Route app-process work through a supported Hermes runtime, including
the app's headless background integration when available, then call the React
Native SDK. Do not initialize a second Swift or Kotlin SDK runtime in that same
process. Persist the validated hint if JavaScript cannot run, then sync when the
OS next permits execution. A push does not authorize a new installation: reopen
the correct current database and use the existing installation. Do not create a
new signing key in a background handler.

On Android, complete the provider callback within its budget. Longer work needs
the app's supported background scheduling integration. On iOS, enable the
required push entitlement and background remote-notification mode in the app
target. Finish the OS completion handler even on timeout or SDK failure.
Neither system guarantees delivery or JavaScript execution after suspension or
termination.

### iOS notification extensions

A notification extension is a separate native process. Metro and the app's
React context do not provide a JavaScript runtime inside it. Do not call React
hooks or assume the app TurboModule exists in the extension.

The React Native npm product provides the app TurboModule. It does not expose a
Swift or Kotlin client facade for a native extension or a handler that runs
without JavaScript. Link the [Swift package](/get-started/install/#swift-package)
into the separate iOS extension process. The native SDK in that process owns
its own client and runtime. A native-only main-app or Android cold-start path
that mixes a sibling SDK with the React Native runtime is not a verified
integration. Use the app's supported Hermes/headless runtime for those handlers.

An iOS extension that decrypts messages also needs an App Group container for
shared current storage and a keychain access group for the saved database key. The app owns these entitlements and access
policies. Close one database owner before another process opens the same files;
do not open the app's active database concurrently from an extension. Within the separate iOS extension process, use this order:

1. Read the saved public identity, database location, and key from app-owned
   storage. Use the native SDK's `build` operation to reopen the current
   installation without generating or registering a new identity.
2. Validate the hint, then run `catchUpToLive` with a timeout inside the remaining
   OS budget. Check the summary's `completed` and `failed` values. An incomplete
   result needs later foreground or scheduled processing.
3. Read the conversation and messages from local storage. Apply consent,
   sender, and display-deduplication rules.
4. Await `end()` to release the client, then complete the native callback.

For an iOS extension, invoke its content handler after releasing the client. Always
provide a fallback when its time budget expires. The silent APNs payload above
does not by itself request a notification service extension; alert and extension
routing must be configured by the app's native notification integration.

See [React Native and Expo](/get-started/react-native/) for native setup and
[Create a client](/sdk/client/) for storage ownership.

## DM stitching

Different installations can create separate groups for the same DM. The SDK
presents these groups as one visible DM. It checks every underlying active group
for notification subscriptions, using each group's consent and override rules. A DM lookup can return a different group ID as the underlying
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
