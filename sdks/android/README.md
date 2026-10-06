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
