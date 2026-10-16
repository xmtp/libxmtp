# XMTP Messenger

This is an Android example for the current `uniffi.xmtp_sdk` package. The Android
host owns the client and device services. Compose screens and app records are in
`shared/src/commonMain`. Open `apps/example-android` as the Gradle project.
The `:example` host is in `app`. The local composite build uses the SDK library
from this checkout. The SDK root owns the shared toolchain pins, wrapper, and
dependency verification inventory.

This app is for testing and debugging purposes only.

## Code structure

The app keeps two Gradle modules. `app` owns Android and SDK integration.
`shared` owns display records, typed actions, screen components, and fixed
preview data. Shared code does not hold SDK objects or Android services.

`ExampleApp` constructs one `AppGraph` per process. The graph supplies these
owners through constructors:

| Path under `messenger` | Responsibility |
| --- | --- |
| `app` | Construct owners and validate navigation. Save route and profile IDs. |
| `session` | Own client lifetime, sequential readers, and cleanup order. |
| `feature/connect` | Inspect server capabilities and connect a profile. |
| `feature/conversations` | Load chats and requests; create conversations. |
| `feature/chat` | Own composer drafts, send state, history, and message actions. |
| `feature/group` | Edit group settings, members, and application fields. |
| `feature/settings` | Own app settings, sign out, and local reset actions. |
| `feature/handles` | Check, suggest, and claim a handle in onboarding and Settings. |
| `feature/attachments` | Own transfers, verified files, and pending device requests. |
| `data/xmtp` | Implement query interfaces and required message projections. |
| `data/profile` | Replay durable profile cleanup. |
| `platform` | Bind device results to a session and navigation request. |

`MessengerViewModel` connects these owners to the Activity. It translates the
screen API and combines display state. It does not start another message reader
or own SDK caches. `MainActivity` launches device requests and hosts Compose.
Activity replacement keeps the graph, transfers, and composer drafts.

Each feature exposes immutable state and typed actions. Add feature behavior to
its controller and query interface. Add layout to the matching shared screen.
The fixed feature fixtures let a screen render without a client or backend.
Keep temporary secrets out of route state, logs, and fixtures.

### Send and restore

A text send first saves a small attempt record. The SDK then saves the message
locally with `optimistic = true` and returns its ID. The app records that ID and
publishes the same stored message. Publication retry uses the original ID.
The attempt record contains references and send phase, not the message body.
Publication success removes it.

The SDK write and app record write are separate operations. If the SDK returns
an ID but the app commit fails, the controller retains that ID and completes the
app commit later. It must not create a second message for the same attempt. If
the ID is unknown after an interruption, the app preserves the unknown outcome.
It does not automatically send the text again.

Composer text uses the encrypted profile store. A completed send can clear only
its matching request and draft revision. A newer edit stays in the composer.
Saved navigation holds IDs only. Restore checks the active profile and native
conversation before opening a chat.

The default reader awaits the required message handler before it reads the next
message. Keep this path sequential: the next SDK read acknowledges the previous
message. Screen replacement must not remove the handler.

### Tests

Prefer installed integration journeys for user features. They should exercise
the actual graph, controllers, SDK database, and shared screen. Use narrow tests
for distinct storage, concurrency, and privacy rules. Test observers are fixed
when the graph is constructed and always preserve the real handler call.
When a journey replaces a narrow test, retain its unique assertions before
removing the old test. Check each new assertion with a plausible weaker
implementation and restore the source after the failure check.

## Run locally

With the backend running, start the app from the repository root:

```sh
dev/nix-shell 'just example-android start'
```

This command builds the local SDK and Debug app, boots a visible API 34 emulator
at 1080 × 2400 and 420 dpi with automatic GPU rendering,
forwards the backend and object-store ports, and opens XMTP Messenger. Use the
backend URL shown in the app. Keep the terminal open. Press Enter to stop the
emulator. This session uses a temporary device; stopping it removes its app data.
The command uses the current worktree ports and keeps the backend running.

## Build and check

Build a sideloadable testing APK from the repository root:

```sh
dev/nix-shell 'just example-android apk'
```

The command prints the output path:
`apps/example-android/build/apk/xmtp-messenger-local.apk`. It builds the
`example-android-apk` Nix derivation, which includes the local SDK's arm64-v8a,
armeabi-v7a, x86_64, and x86 libraries in one signed release APK.
It does not need a backend or emulator. Android 8.1 (API 27) or newer is required.
Transfer the APK to a device and open it, allowing installation from that source
when Android asks. Or install it over USB with `adb install -r <apk-path>`.

Every Android dev, RC, stable, and nightly release attaches
`xmtp-messenger-<version>.apk` to its `android-<version>` GitHub Release.
These APKs use the separate `org.xmtp.android.example.testing` app ID and show
as **XMTP Messenger (Testing)**. They use the public AOSP testing key so local
and CI testing builds can update each other without replacing normal app installs.
Their profiles and local data stay separate. They are for testing only. Remote
servers require HTTPS; HTTP is limited to loopback. Firebase notifications are off
in the downloadable APK.

You can also build directly with Nix on macOS ARM64 or Linux x86_64:

```sh
nix build .#example-android-apk
```

The APK is at `result/xmtp-messenger.apk`. Its version comes from the Android SDK's
`gradle.properties`. Nix can cache the finished APK. The build uses pinned Gradle
downloads and does not use your local Gradle cache or private configuration.
After changing dependencies, refresh the download list with
`dev/nix-shell 'just example-android apk-update-deps'`, then build again.

Choose a reachable HTTPS server in the app. For a USB-connected device and a
local backend, run `adb reverse tcp:5050 tcp:5050` and enter
`http://127.0.0.1:5050`; replace 5050 with the port from `just backend status`.
Forward the object-store port too when testing attachments. The default
`10.0.2.2` address reaches the host only from an Android emulator.

Server checks stay quiet until you press Join. The Access token field accepts
either a raw token or a complete `Bearer <token>` value.

Run these commands from the repository root:

```sh
dev/nix-shell 'just example-android lint'
dev/nix-shell 'just example-android check'
dev/nix-shell 'just example-android test'
dev/nix-shell 'just backend up'
dev/nix-shell 'just backend status'
dev/nix-shell 'just example-android test-integration'
```

The integration recipe owns its emulator and stops it after the tests. It uses
the current worktree backend and object-store ports. SDK package, consumer and
process lifecycle checks remain separate gates.

## Connect

The Join a server screen shows the backend URL from the worktree build configuration.
You can edit it. Use HTTPS for a remote backend. HTTP is allowed only for the
local development hosts `localhost`, `127.0.0.1`, `[::1]` and the Android emulator
host `10.0.2.2` in debug builds. Release builds permit HTTP only for the listed
loopback hosts. Do not put credentials in the URL.

Release builds, including the downloadable testing APK, behave as follows:

- They are not debuggable, and `BuildConfig.DEBUG` is false.
- They reject an HTTP URL for `10.0.2.2` or a remote host with "Use HTTPS for a
  remote backend". The app checks the URL before it saves a profile, stores a
  credential, or starts the SDK. The same check applies when you connect, when
  the app restores a saved profile, and when it inspects a server.
- Their network security policy blocks cleartext traffic except to `localhost`,
  `127.0.0.1`, `::1`, and `[::1]`. A server on the device itself can use HTTP.
- No device test runs a release build. Host tests check the release URL rules:
  `dev/nix-shell 'just example-android test :example:testReleaseUnitTest'`.

The Credential field appears only when the selected server reports that it
requires authentication. Enter that server's credential in this field. If a
connection fails, Retry uses the current URL and credential. A failed saved
connection keeps its saved credential when the field is empty.
A result for an earlier URL cannot change that field.

The app retains one hidden wallet per backend profile. Private key, credential
and database key records use Android Keystore encryption. App and SDK files are
excluded from backup.

An old example database with no retained wallet key shows a migration state.
Select its inbox only when you want to reset that local account. First launch
does not remove an old database or select another inbox's database.

Sign out stops the client and clears its credential. It keeps the wallet and SDK
files. Delete my account removes the selected profile's local database, SDK
attachment files, temporary sources, exported files and app records. This is a
local device reset. A failed reset retains its cleanup record for Retry.

## Handles

A handle such as `nick` or `nick#2341` is a public name that people on the
server use to find and message you. After Join a server, a new account opens
Choose your handle. The step runs once for each account. Skip for now opens
Chats. An account whose handle this device already knows does not see the step.

As you type, the app checks the handle grammar and the server's published
minimum name length. Continue asks the server which handles are free. A free
name shows Claim. A taken name shows three free `name#code` handles, and Show
more adds three more. The app claims only the handle you pick. When another
inbox claims your pick first, the app asks the server again, shows the free
handles with the first one selected, and waits for you. The server decides
each claim: a refused name, a claim within 7 days of the last one, and the
64-claim limit each show as a message. A successful claim opens Chats.

Settings shows your current handle as the profile title and your other handles
below it. Change handle reuses the editor, lists your other handles with Use,
and asks for confirmation before each claim. If you type one of your other
handles, the editor asks to switch back to it, as Use does. If you type your
current handle, the editor says so. A switch back to an older handle
is a new claim, so it must pass the server's current rules. The app records the
time of each claim it makes, so it can show when you can change again. A claim
from another device has no date on this one.

## Chat

Chats shows accepted conversations. Message requests shows conversations with
Unknown consent. Accept applies consent in one step. Delete request and Block
require confirmation. Delete request dismisses the local entry and keeps native
conversation data. Block changes native consent to Denied.
Accept is required before composer, send, and outbound message or reaction
actions. Copy, reactor lists, and Close remain available. Retained outbound
callbacks also require accepted consent.
New conversations and Add member accept handles, Ethereum addresses, or inbox
IDs. The app detects the input type: a `0x` prefix selects Ethereum, 64 hexadecimal
characters select an inbox ID, and a handle resolves through the SDK.
Handle input accepts an optional `@` prefix.
Selected people, DM headers, conversation rows, and member details show the
current handle when available, with the inbox ID as a fallback. DM titles show
handles with an `@` prefix.
Groups offer All members and Admins only presets.

Messages support text, replies, emoji reactions and remote deletion. “Sent”
means that the SDK reports Published. It does not mean that a recipient read the
message. The composer keeps text until the SDK accepts a message ID. A definite
failure before acceptance keeps the draft. New edits stay in the composer when
an earlier send completes. Publication Retry names the accepted message ID. A send interrupted
before its ID is saved has an unknown outcome. Settings offers View chat and
Discard record. It does not resend that record automatically.

Group settings use current names, descriptions, roles, built-in permissions and
disappearing settings. A preset change uses separate SDK writes. After the first
write starts, navigation does not stop the remaining writes to that group while
the same session is active. A partial SDK failure shows the state read from the
SDK. Request removal shows PendingRemove
until a later committed state changes it.

## Conversation fields

Conversation details opens Group fields and My fields. Group fields shows the
registered application fields. My fields shows your values and member values
for this conversation. String values use text. Bytes use hex. Map and set
controls change one entry. A save in My fields sends only changed own values in
one commit. Set empty keeps a value; Clear removes it.

The committed registry supplies field types and policies. An unknown type or
policy shows Unsupported. An offered field that is absent from the registry
shows a missing-field notice. The app cannot register arbitrary fields.
Descriptors and values refresh after a mutation, metadata event, event lag,
foreground return, or session open. An old screen or session cannot publish a
save result into the current editor. SDK failures retain their actual type.

The [catalogue fixture](fixtures/README.md) supplies stable example group and
user fields. The integration recipe starts its own backend and database. It
keeps the shared backend catalogue unchanged.

## Local history limits

Published history uses SDK pages of 50 raw rows. The SDK orders by sent time,
then local delivery sequence. The app preserves that order and uses opaque SDK
positions for older and newer queries. A short or empty converted list does not
prove completion. Raw continuation can advance past unreadable rows. A notice
shows conversion loss. Each operation uses at most four history reads; Load more
continues from the last consumed raw position.

The cache keeps three transcripts and at most 500 published rows per transcript.
Queued and failed messages have a separate 50-row recovery page. Use Older pending
messages and Newest pending messages to change this page. Refresh reads the selected
page again. Recovery uses sent time and an opaque message-ID boundary, so tied rows
remain reachable. Each recovery operation uses at most four SDK page reads. Raw
continuation can advance through an empty converted page. A conversion-loss notice
keeps the next pending page available.
Saved positions retain the message key, pixel offset and SDK tuple. Refresh
reads the window around that tuple. A deleted anchor still has a valid query
boundary. The app selects the next surviving newer row, then an older row, at
offset zero and shows Position changed. A rejected cursor clears the saved
position and opens the newest page. Published ties have no timestamp-bucket stop.

Unread counts use retained incoming Published text, Markdown, reply and remote attachment
rows with an insertion timestamp greater than the local read marker. Equal
timestamps count as read together. Imports can retain an older insertion time.
The count and marker reads are separate SDK calls.

Attachment, custom field and notification controls are enabled only when their
app feature wiring is present. Shared UI records contain no SDK or Android
objects.

## Optional notifications

The app builds and runs without Firebase configuration. App settings shows
notifications Off. This build does not ask for Android notification permission,
request an FCM token, or register SDK notifications.

To enable the Firebase build, supply your Android app's `google-services.json`:

```sh
XMTP_FIREBASE_CONFIG=/absolute/path/google-services.json dev/nix-shell 'just example-android check'
```

You can also keep the file at `apps/example-android/app/google-services.json`. Git
ignores that file. The application ID must be `org.xmtp.android.example`.
The configured dependency graph has its own strict lock file. It uses Firebase
Messaging 26.0.0 and Fragment 1.8.5 with the current Android toolchain.
The backend must also have its FCM channel configured.

Open App settings and select Turn notifications on. This action asks for Android
permission when required. The app shows the current token, permission and SDK
registration state. Conversation details can enable or mute the conversation.
The chat list shows Muted only for an explicit per-conversation mute choice.
Sign out persists signed-out state before stopping owned jobs and unregistering.
A failed unregister cannot reopen the signed-out account.

Notifications show only “XMTP Messenger” and “You got a message.” A group topic
must resolve through the current profile's local SDK database. Unknown groups,
Denied consent, muted conversations, app Off, and stale sessions are dropped.
A Welcome must name the current installation and opens the conversation list.
Topic and unsigned sequence identify a notification, not a message. The app does
not fetch an exact message from that pair or substitute the latest message.

A background push can reopen only the saved signed-in profile with its recorded
SDK database still present. This path starts no default message collector.
Foreground launch adopts that process owner and starts normal collection.
A saved account cannot replace a pending sign-out or reset. A user Connect
request starts the next session generation.

When Firebase or the app setting is Off, an existing SDK database first opens
with the task runner disabled. This temporary client clears stored notification
state before a normal worker can renew it. Local Disabled state survives an
unavailable Unregister response. The temporary client then ends, and the app
opens its normal client with the original worker options. Chat workers remain
available in that normal session.

The persisted-state fixture uses the local backend's HTTP channel with an empty
conversation selection and no Welcomes or sync groups. It does not deliver to
that target or register with FCM. Other local tests use a simulated configured
transport and registration state with
real app session, SDK conversation, consent and preference paths. They do not
prove Firebase delivery. Real background receipt and tap testing still require
a developer Firebase project and a device. Synthetic public build resources
can prove configured compilation only.
