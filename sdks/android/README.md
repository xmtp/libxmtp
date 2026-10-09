# XMTP Android SDK 8.0.0

The Android SDK uses the generated `uniffi.xmtp_sdk` API. Rust owns messaging,
standard codecs, attachments, notifications, storage, and readers.

```gradle
implementation 'org.xmtp:android:8.0.0'
coreLibraryDesugaring 'com.android.tools:desugar_jdk_libs:2.1.5'
```

```kotlin
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import uniffi.xmtp_sdk.*

val signer = generateLocalSigner()
val client = SDKClient.create(context, signer, ClientOptions(
    backend = BackendSource.Options(BackendOptions(url = backendUrl)),
    storage = StorageOptions(location = StorageLocation.Default),
))
try {
    val group = client.conversations.createGroup(emptyList<InboxId>())
    group.sendText("Hello")
    val messages = group.messages()
} finally {
    withContext(NonCancellable) { client.end() }
}
```

The SDK supports Android API 23 and later. Enable
`android.compileOptions.coreLibraryDesugaringEnabled = true` in the app. The
generated timestamp API uses `java.time.Instant`; desugaring supplies it on
Android API 23 to 25. Both repository examples enable it.

The Context factory stores each deployment and inbox under `filesDir/xmtp_db`.
Use `SDKClient.build(context, identity, options)` to reopen the same identity.
Set `allowOffline = true` and pass the known `inboxId` to permit startup from
stored state when the backend is unavailable. Supply a storage label to keep
separate local instances.

Pass an app-owned 32-byte `encryptionKey` for encrypted persistent storage.
Reuse that key when you reopen the database. Omitting the key selects
unencrypted storage.

Process lifecycle control is on by default. Set
`AndroidStreamLifecycle.enabled = false` before the first Context factory call
to manage the native transport yourself with `resumeStreams()` and
`suspendStreams()`.

Persistent file logging uses the `SDKClient` companion helpers
`activatePersistentLibXMTPLogWriter`, `deactivatePersistentLibXMTPLogWriter`,
`getXMTPLogFilePaths`, and `clearXMTPLogs`. They use `filesDir/xmtp_logs` and the
generated native writer. `maxFiles` is `UInt`. Call the suspend function
`initLogging(LoggingOptions(level = LogLevel.DEBUG))` before the first writer
activation in each process.

8.0.0 changes the public package and types. Replace `org.xmtp.android.library`
and `uniffi.xmtpv3` imports with `uniffi.xmtp_sdk`. Use `SDKClient` and generated
`ClientOptions`, `BackendOptions`, and `StorageOptions`. Standard messages use
`SDKMessageContent.Standard` and typed `MessageContent` records. Register custom
codecs per client through the factory `codecs` argument. Use typed send methods
such as `sendText`, `sendReaction`, and `sendRemoteAttachment`.

The package retains `ByteArray.toHex()` and `String.hexToByteArray()` for hex
conversion. `validateInboxId()` and `validateInboxIds()` check the forbidden
`0x` prefix. SDK operations apply the full ID rules and return typed errors.

Persistent attachments use `client.attachments()`: create a pending attachment,
send its `remoteAttachment()` record, then upload it. A recipient downloads the
record through its own attachment store. The SDK checks transfer limits and
content digests. Local emulator fixtures need `allowPrivateNetwork = true` and
`adb reverse` for the backend's advertised loopback attachment port.

Use `group.streamMessages()` or `dm.streamMessages()` for a Flow. The next native
read acknowledges the previous message after `emit` returns. With direct
sequential collection, the collector callback finishes before
that acknowledgement starts. A buffer or another asynchronous operator can let
`emit` return before downstream processing ends. Cancellation after
acknowledgement does not restore the message to default progress. The Flow does
not provide durable acknowledgements for each downstream consumer.
Cancellation before ACK commit admission preserves the message for replay.

Only one default message reader can own progress in a client database. Different
group or DM scopes do not create separate default owners. A second active default
message reader fails with `XmtpException.ConsumerOwned`. Set the reader option
`from` to an explicit snapshot cursor for independent replay/live reading. These readers do
not advance default progress or create a durable consumer checkpoint. End the
current default reader before opening another default reader.

If the collector throws, its exception propagates to the caller and `onClose`
receives one `Closed` reason. Normal completion and cancellation also receive
one `Closed` reason.
Close clients and readers in `NonCancellable` teardown. See [development rules](AGENTS.md) for build
and test commands. The [example](example) uses the same public API.

## Messenger files

The Messenger example sends one file per message. Select a file in a chat,
then use **Send file**. The app copies the URI into private storage in chunks
of at most 64 KiB. It checks the server upload limit from the actual bytes.
The SDK applies its final limit, which also includes encrypted content overhead.
Upload finishes before the app queues the message. **Delivered** means SDK
publication.

**Settings → Draft recovery** shows retained file drafts. The SDK age limit is
24 hours by default. A retained Complete upload can resume through its encrypted
full descriptor. An expired or missing record shows **Draft expired or
unavailable**. Select a file again to make a new draft. A send interrupted before
its accepted message ID was saved requires **View chat** or **Discard**. The
original message may already exist. Recovery never sends it again automatically.
Publication retries use the saved message ID.

Before queue admission, **Discard** stops native upload work and removes its
local files. Cancelling the coroutine that waits for an upload does not stop
native transfer. Discarding an unknown or accepted send reference removes the
app draft. Message deletion remains a separate chat action.

Use **Download** to obtain an SDK-verified local file. Image previews use a
sampled local bitmap. **Open** copies that file into the selected profile's
private export directory and grants read access to that file URI. **Save**
copies it through the system-selected destination URI. The FileProvider exposes
only `messenger-exports/`. Sign out revokes export grants and clears exports.
Local account reset also revokes grants during cold recovery and removes the
selected profile's owned files.

For local object-store tests, start the backend and read the worktree routes:

```bash
dev/nix-shell 'just backend up'
dev/nix-shell 'just backend status'
dev/nix-shell 'just android example-test'
dev/nix-shell 'just android example-test-integration'
```

The installed app tests use the owned emulator scope. The recipe forwards the
advertised S3 port with `adb reverse`, so signed loopback attachment URLs work.
Enable **Allow local attachment network** for this local fixture. The backend
route can use the emulator gateway; attachment URLs still follow the SDK URL
rules. The interruption tests receive proxy routes from the worktree environment,
hold only their named toxic, and restore the proxy on exit. Run proxy tests alone
when a stack is shared. To use an existing stack from another checkout, source
that checkout's `dev/docker/load-env` before the recipe. Caller values take
precedence over generated worktree defaults.
