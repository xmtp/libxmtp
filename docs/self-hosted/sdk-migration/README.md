# Migrate to the generated SDK

This guide describes the approved generated SDK contract. The public package sources now use generated bindings. Use staged packages to check a migration.
The [cutover handoff](handoff.md) records the checks that must pass before a
package switch. Every switched SDK targets version **8.0.0**. This work changes
no package version and publishes no package.

## Install and import

Use ESM imports on Node and in a browser. Node requires **22.12 or later**.
Replace `require`, CommonJS entry points, and deep binding imports. The new SDK
has no CommonJS adapter and no legacy codec adapter.

The staging package names are `xmtp-sdk` for Node and `xmtp-sdk-browser` for the
browser. Browser pure helpers use `xmtp-sdk-browser/pure`. These names identify
local test products. The Phase 2 package owners keep the approved public package
names when they switch their contents. Swift imports `XmtpSdk`. Kotlin imports
`uniffi.xmtp_sdk.*`. Use `SDKClient` on Swift and Kotlin and `Client` on TypeScript.
Do not access `.raw` or import a private binding path.

## Change names directly

The generated public declarations define the final names. The new major
version has no rename aliases. The temporary cutover inventory is retired.
These common changes apply on every host; native object members are methods.

| Old form                                                | Swift and Kotlin                                        | Node and browser                                |
| ------------------------------------------------------- | ------------------------------------------------------- | ----------------------------------------------- |
| `inboxID`, `inboxId`                                    | `inboxId()`                                             | `inboxId`                                       |
| Nanosecond timestamp integers and separate date helpers | `Timestamp.ns`, `Timestamp.date`                        | `Timestamp.ns` (`bigint`), `Timestamp.date`     |
| Client database path                                    | `storage().path()`                                      | `storage.path()`                                |
| Delete local database                                   | `storage().delete()`                                    | `storage.delete()` on Node; excluded in browser |
| End a client or reader                                  | await `end()`                                           | `await end()`                                   |
| Delicate account and signature calls                    | `unsafeAddAccount`, `unsafeCreateInboxSignatureRequest` | Same names                                      |
| Legacy reaction v1 codec                                | app codec or `ReactionV2Codec` for v2                   | Same rule                                       |
| Global codec registration                               | codecs on `SDKClient.create` or `build`                 | `options.codecs` on `Client.create` or `build`  |
| Awaited app-data merge callback                         | metadata reads and typed events                         | Same rule                                       |
| `DecodedMessageV2`                                      | `Message`                                               | `Message`                                       |

String IDs keep their exact contents. The SDK checks IDs in Rust. TypeScript
64-bit values are `bigint`; do not convert them to `number`. `canMessage` map
keys include identity kind: `ethereum:<validated text>` or
`passkey:<lowercase hex>`. Read the returned key; do not strip its prefix.

The old active-only sync flag is absent. Consent filters do not replace
active-only filtering. The new Dm API has no permission-policy getter. Use the
approved new API without these old getter and flag forms.

The old standalone delete-payload codec is removed. Use the typed message
delete action and read its typed result. The surviving
`message_actions_use_ids_and_compression_is_opt_in` test checks this route.

## Keep the existing database

End the old client before opening the new client. An app must never open both
clients on one database at the same time. Keep the existing encryption key on
native hosts. Swift migration helpers accept the caller-stored 32-byte
`databaseKey: Data` and pass it to both opens. Reuse the same key for Default,
Directory and Explicit storage. Omitting `encryptionKey` selects unencrypted
storage. Changing the default location does not move an old database.
Use the old client's reported database path for the first new open.

| Host    | Old default database                                                            | New default root                              |
| ------- | ------------------------------------------------------------------------------- | --------------------------------------------- |
| Swift   | `Documents/xmtp-{env}-{inboxId}.db3`, or the old `dbDirectory`                  | `Application Support/{bundleIdentifier}/xmtp` |
| Android | `{context.filesDir}/xmtp_db/xmtp-{env}-{inboxId}.db3`, or the old `dbDirectory` | `{context.filesDir}/xmtp_db`                  |
| Node    | `{creationCwd}/xmtp-{env}-{inboxId}.db3`, or the old `dbPath` / callback result | `{creationCwd}/xmtp`                          |
| Browser | OPFS entry `xmtp-{env}-{inboxId}.db3`, or the old `dbPath`                      | OPFS pool entry `xmtp-sdk`                    |

For `Default` or `Directory`, an optional non-empty storage label adds one child
directory to the root. This is `data_dir`. The complete layout is:

```text
{data_dir}/{deployment}/{lowercaseHexInboxId}/xmtp.db3
{data_dir}/{deployment}/{lowercaseHexInboxId}/attachments/
```

The deployment component uses the bound configuration identifier. Apply
[ATCH-040](../../specs/ATCH-remote-attachments.md#5-local-files)'s filename
steps 2–8, with a 190-byte limit in step 6. Lowercase ASCII. Append `-` and the
64 lowercase hex digits of SHA-256 over the original identifier's UTF-8 bytes.
Do not substitute the environment label or backend hostname.

Use `Explicit { dbPath, attachmentsDir }` to reopen the actual old file.
Both paths are required. Native hosts resolve relative paths against the current
working directory when the client opens. `storage.path()` returns that absolute
path. Normalize both native paths before you compare them. Keep the working
directory fixed between opens. Browser OPFS names stay unchanged.
Protect native `dbPath`, `attachmentsDir`, and their parent directories from
changes by other local users. The SDK follows caller-supplied paths, including
symlinks. Do not use a working directory that other local users can change.
The SDK adds no label, deployment, or inbox directory. It needs no
path-discovery request or caller
inbox ID. Choose the attachments directory yourself; the old database path
alone is insufficient. Browser paths name OPFS entries, including attachment
`Path` sources. A host filesystem path cannot identify a browser file.
On native hosts, attachments are plaintext files. Use an app-owned directory
that other local users cannot read or change. The SDK does not tighten the
permissions of an explicitly supplied attachments directory.

Labels cannot be `.`, `..`, or contain `/`, `\`, `:`, or NUL. An unsupported
platform or Apple process without a bundle identifier must name a directory or
explicit paths. `InMemory` has no inferred attachments directory. Messaging and
pure encryption/codecs work; a transfer that needs local files fails with the
typed `local_storage` cause. Storage deletion removes only the database file.
Attachments and the deployment record remain until the app removes them.

## Create, build, and start offline

`create` uses a signer and can establish an identity. `build` takes a public
identity and opens an existing stored identity. On an empty database, `build`
fails with `IdentityNotFound`; it does not register an installation. An identity
that does not belong to the opened inbox fails with `IdentityMismatch`.

| Host             | Create                                            | Existing identity only                              | Offline option                       |
| ---------------- | ------------------------------------------------- | --------------------------------------------------- | ------------------------------------ |
| Swift            | `SDKClient.create(signer:options:codecs:)`        | `SDKClient.build(identity:options:inboxId:codecs:)` | `options.allowOffline = true`        |
| Kotlin           | `SDKClient.create(signer, options, codecs = ...)` | `SDKClient.build(identity, options, codecs = ...)`  | `options.copy(allowOffline = true)`  |
| Node and browser | `Client.create(signer, options)`                  | `Client.build(identity, options, inboxId?)`         | `{ ...options, allowOffline: true }` |

`allowOffline` defaults to false. With default/directory storage, offline path
lookup uses the deployment identifier saved for that backend URL and requires
the saved `inboxId` argument to `build`. Without a saved mapping it fails with `StorageLocation` and sends no request. Use explicit
paths to start offline without that mapping. Stored configuration permits
reopen after a URL change; the SDK checks configuration before its first later
request. An offline open does not prove that a later backend is compatible.

On Android, resolve the storage location with the app context before either
factory call. `StorageOptions(context, label = options.storage.label)` selects
`File(context.filesDir, "xmtp_db").absolutePath`. Copy only its location into
the caller's storage record. Keep the caller's encryption key, label, pool, and
single-connection settings for both `create` and `build`. The Kotlin migration
example opens an existing public identity with `build` and passes its saved
inbox ID for the default path. A bare
`StorageLocation.Default` requires the factory
`defaultDirectory = File(context.filesDir, "xmtp_db").absolutePath` argument
on both calls. It cannot find the app context on its own.

The complete source examples are [Node](node.ts), [browser](browser.ts),
[Swift](Migration.swift), and [Kotlin](Migration.kt). Each ends the first client
before reopening and checks the reopened path and inbox. Supply the public
identity from the legacy database, backend options, and the actual persistent
path. The default Android path also needs the saved inbox ID. These examples
use `build` for both opens; they do not register a new identity. Compiler and runtime results are separate
checks in the handoff.

## Select messages and save progress

The all-conversation message reader defaults to `allowed` plus `unknown`
consent. An explicit consent list replaces that default. `[]` selects no
messages. An absent conversation kind selects both Group and Dm; a supplied
kind selects only that kind. A named Group/Dm reader has no consent filter.
The selection is fixed for the reader's life. The final handoff check uses
current conversation state. New matching conversations are included.

Use `MessageReaderOptions` with `conversationKind`, `consentStates`, and `from`.
Swift uses `nil`, Kotlin uses `null`, and TypeScript uses `undefined` for an
absent option. Their explicit empty lists are `[]`, `emptyList()`, and `[]`.
Do not use a truthiness check that replaces an empty list with defaults.

A full stored `Message` has `deliveryCursor` when it has a committed delivery
number. In TypeScript the absence is `null`; native hosts use an optional.
Optimistic or failed messages can have no cursor. Save a cursor only **after
successful app work**. Deduplicate repeated work by message ID where needed.
Do not save the cursor in a `finally` block after failed work.

Opening with `from` replays strictly after that cursor. It does not change the
default reader's durable progress or take its consumer lease. Replay is not an
acknowledgement. A cursor belongs to one database. Whole-database restore/import
rotates its identity; additive archive import preserves it. Use the cursor
unchanged. `InvalidCursor`, `ForeignCursor`, and `ConsumerOwned` are separate
failures. A history query plus a cursor read is not an atomic, gap-free
history/live snapshot.

| Host             | Old consumption call                         | Supported new consumption                                                            |
| ---------------- | -------------------------------------------- | ------------------------------------------------------------------------------------ |
| Swift            | `group.streamMessages` / `streamAllMessages` | `client.messages(in: group)` / `client.messages()`; `for try await`                  |
| Kotlin           | `group.streamMessages` / `streamAllMessages` | `client.messages(group)` / `client.messages()`; Flow `collect`                       |
| Node and browser | `conversation.stream` / `streamAllMessages`  | `MessageStream.openGroup(client, group)` / `MessageStream.open(client)`; `for await` |

For supported iteration, use the existing host adapter. Swift uses
`let stream = try await client.messages(in: group)` and `for try await message
in stream`. Kotlin uses `client.messages(group).collect { ... }`; `first()` or
`take(1).toList()` ends early. Node/browser use
`MessageStream.openGroup(client, group)` with `for await`. Loop exit ends the
reader automatically. Swift iterator destruction triggers end but cannot await
teardown. Existing native reader `end()` remains the explicit awaited close;
no new Swift iterator `end()` method is added. Kotlin Flow finalization awaits
end even on cancellation. Node/browser `await stream.end()` joins teardown.
The reader lane has focused proof with real reader leases. The final integrated package and
platform checks remain pending in the handoff. Keep raw `next()` outside
supported app consumption. An iterator's next
request acknowledges the prior item; a successful callback return acknowledges
that item. Failure, cancellation, or loop exit must preserve the approved
acknowledgement contract. Automatic end does not add an acknowledgement for the
last yielded item. Explicit client shutdown still uses `await end()`.

## Write a new codec

Import `ContentCodec<T>`, `EncodedContent`, and `ContentTypeId` from the new
SDK's public root. Swift uses a `ContentCodec` protocol with its value type;
Kotlin and TypeScript use `ContentCodec<Value>`. Do not import legacy
`@xmtp/content-type-primitives`, generated protobuf types, or a legacy adapter.

`encode` and `decode` are synchronous. `encode` returns a required type ID,
parameters map, content bytes, and optional fallback. The codec can supply
`fallback` and `shouldPush` hooks. Register codecs per client. The mixed receive
registry erases the value type; typed `send`, `prepareMessage`, and `reply` keep
it. The existing installed codec consumers show all four host forms:

- [Node codec](../../../crates/xmtp_sdk/conformance/public/node/src/codecs.ts)
- [Browser codec](../../../crates/xmtp_sdk/conformance/public/browser/src/codecs.ts)
- [Swift codec](../../../crates/xmtp_sdk/conformance/public/swift/Sources/PublicConsumer/Codecs.swift)
- [Kotlin codec](../../../crates/xmtp_sdk/conformance/public/kotlin/consumer/src/main/kotlin/Codecs.kt)

For standalone browser codecs, import from `xmtp-sdk-browser/pure` and await
`initPureWasm()` before you construct `TextCodec`, `ReactionV2Codec`, or another
standard codec. The [browser example](browser.ts) shows this order and checks
both codec round trips. A client open does not replace this pure-module setup.

History content-type filters accept supported standard type IDs. The old
`ContentType.Custom` wildcard is removed. The CLI also removes the `custom`
value from `conversation messages --content-type` and
`--exclude-content-type`. The CLI rejects that value. It does not return an
unfiltered history.

The standard catalogue owns push defaults. Read receipts, reaction v2, group
updates, group membership changes, deletion, leave requests, and edits default
to false. Other catalogue entries default to true. A custom type defaults to
true when it has no hook. An explicit `SendOptions.shouldPush` wins. A custom
codec cannot change a catalogue type's default through its hook. This replaces
the old blanket Node false and browser true defaults.

## Handle content and transfer errors

Every fallible SDK operation keeps `ErrorDetails { code, category, retryable,
message }`. Codes use stable PascalCase. Swift catches `XmtpError`, Kotlin
catches `XmtpException`, and TypeScript catches public `XmtpError` subclasses.
Use typed cases and `code`; display text is not a classifier. The Node and
browser source examples include `describeMigrationError` with a default case. Keep a default
case for a code added in a later release. Log its code and retryability and
surface the failure to the app. Do not turn an unknown code into success.

The retained-content contract preserves exact `Message.rawBytes` and reply
parent raw bytes. Received encoded content and received type fields can be
absent. Do not invent empty bytes or an empty type ID. Unknown content carries
typed details: `CodecNotFound` for a missing custom codec, `CodecDecodeFailed`
for standard, nested, or compression failure, and `MalformedEnvelope` for invalid
or untyped envelopes. A registered custom decoder that throws produces
`CodecDecodeFailed` with callback category and `retryable = false`. Failed
encoding or send hooks produce `CodecEncodeFailed` before publication. A bad
message does not stop later message delivery. The F6 final packages must prove
raw-byte, absence, and decompression failure behavior before cutover.

Attachment errors and Failed status preserve the same failure record: cause,
retryability, credential detail, missing scope, and available HTTP status.
Keep `ClientClosed` as a lifecycle error. No automatic transfer retries are
added. `Bytes` sources copy at the browser boundary; `Path` sources read OPFS.
The app supplies and displays metadata images and previews. A metadata field
or image URL does not cause the SDK to create an image preview.

## Callback shutdown

A credential callback must return before its owning client is ended. Awaiting
`Client.end()` on that same client from inside `CredentialSource.credential`
is unsupported. An external caller can end the client while the callback is
held; release the callback so shutdown can complete. A callback can await
`end()` on an independent client. Keep those two supported cases separate.

A callback that never returns does not have a thread-release guarantee. The
final lifetime gate proves cleanup after completed callback cycles. Event
callbacks can reenter their listener stop and client end; those operations do
not wait for that active event callback.

## Compression and logging

Compression is opt-in through `SendOptions.compression` (`Deflate` or `Gzip`).
Omission is uncompressed. Deflate uses zlib format; gzip uses RFC 1952. Receive
paths decompress automatically within CTYPE bounds. Old iOS LZFSE bytes marked
as gzip are not a supported format. Preserve a failed decode's received bytes.

Install one async `LogSink.log` through `setLogSink`. Each host awaits the sink
callback. The queue allows at most 4096 waiting records and one active callback
across generations and transports. Overflow drops new records. Sink replacement
drops old queued records and returns without waiting for an active old call.
The next generation waits for that active call before its first callback. A slow or failed app sink does not block SDK work.
There is no final sink flush guarantee on replacement or client/process end.
`flushTelemetry` does not flush the app log sink. Remove `setLogSinkQueued` and
`LogWindow` usage. Treat logs as diagnostics, not as a durable app event stream.
