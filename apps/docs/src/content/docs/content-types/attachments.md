---
title: Support attachments in your app built with XMTP
---

Use the remote attachment, multiple remote attachments, or attachment content type to support attachments in your app.

- Use the [remote attachment content type](#support-remote-attachments) to send one encrypted file through storage.
- Use the [multiple remote attachments content type](#support-multiple-remote-attachments) to send several encrypted files in one message.
- Use the [attachment content type](#support-small-inline-attachments) to send small files inline.

## Support remote attachments

Use a remote attachment to send a file stored outside the message. The backend must offer attachment storage for SDK-managed uploads. An app can also use its own storage through an upload callback.

To send several remote attachments in one message, see [Support multiple remote attachments](#support-multiple-remote-attachments).

## Native attachment uploads and downloads

The SDK has built-in file transfers through `client.attachments` on Browser and
Node, and `client.attachments()` on Kotlin and Swift. These calls encrypt,
stage, upload, verify, and decrypt files. An upload callback is not required.
The Agent SDK uses the same transfer service for `ctx.sendRemoteAttachment`.

For uploads, the operator must configure
[attachment storage](/deploy/attachment-storage/). Check `offered()` before you
show the upload action. Downloads can use any permitted HTTPS host, including
another deployment or app-owned storage. They do not require an attachment
offer from the recipient's backend.

### Create, send, and upload

1. Call `create` with a path or bytes, a MIME type, and an optional filename.
2. Get the `RemoteAttachment` from the returned pending attachment.
3. Send that record with `RemoteAttachmentCodec`.
4. Call `pending.upload()` and handle its result.

Creation reads the source once and stores both a local plaintext file and
staged ciphertext. It returns before any upload request. After creation, the
source can move or be removed without affecting the upload. Prefer a path
source for large native files. Path creation and transfers stream the file;
a byte source already holds the file in application memory.

The following examples use an existing `client` and `group`. The path must name
a readable file in the application's filesystem.

#### Kotlin

```kotlin
val attachments = client.attachments()
check(attachments.offered()) { "The backend does not offer attachments" }

val pending = attachments.create(
    AttachmentSource.Path(
        path = "/path/to/photo.jpg",
        filename = "photo.jpg",
        mimeType = "image/jpeg",
    ),
)
val remote = pending.remoteAttachment()
group.send(RemoteAttachmentCodec(), remote)
pending.upload()
```

#### Swift

```swift
let attachments = client.attachments()
if attachments.offered() {
    let pending = try await attachments.create(source: .path(
        path: "/path/to/photo.jpg",
        filename: "photo.jpg",
        mimeType: "image/jpeg"
    ))
    let remote = pending.remoteAttachment()
    _ = try await group.send(RemoteAttachmentCodec(), value: remote)
    try await pending.upload()
}
```

For an in-memory file, use `AttachmentSource.Bytes(bytes, filename, mimeType)`
on Kotlin or `.bytes(bytes:filename:mimeType:)` on Swift. Browser path sources
name OPFS entries. Browser byte sources copy the caller's bytes at the SDK
boundary.

Sending the record does not start the upload. With the sequence above, a
recipient can receive the message before the file is available. Show a pending
state and allow another download attempt if the first returns `not_found`.
An app can also complete the upload before sending when it needs the object
to be available first.

### Download to a local file

Decode the received content with `RemoteAttachmentCodec` to get a
`RemoteAttachment`. Pass that record to `download`:

```kotlin
val downloaded = client.attachments().download(remote)
val path = downloaded.path
val mimeType = downloaded.mimeType
val filename = downloaded.filename
```

```swift
let downloaded = try await client.attachments().download(remote: remote)
let path = downloaded.path
let mimeType = downloaded.mimeType
let filename = downloaded.filename
```

`DownloadedAttachment.path` names a file with the original content bytes, not
the encoded envelope or ciphertext. The returned MIME type and filename come
from the decrypted envelope. Use the local path to open or display the file.
The SDK does not create a preview or thumbnail.

The SDK checks the ciphertext digest, decrypts and authenticates the envelope,
and checks its attachment type before it puts the plaintext at its final path.
A failed download leaves no partial file there. If a file already exists at
that path, the SDK returns it without a network request. Receiving a message
does not download its attachment. Request downloads when the user opens the
file, or when your app explicitly chooses to fetch it.

### Status, retry, and local storage

| Call                  | Use                                                                                       |
| --------------------- | ----------------------------------------------------------------------------------------- |
| `pending.status()`    | Read `waiting`, `uploading`, `complete`, or `failed`. A failed status includes its cause. |
| `listPending()`       | Find incomplete uploads after an application restart.                                     |
| `pending(remote)`     | Recover the pending handle for a known remote attachment.                                 |
| `pending.upload()`    | Start an upload, join a running upload, or return success for a completed upload.         |
| `localPath(remote)`   | Derive the local path without fetching the object or checking that the file exists.       |
| `listLocal()`         | List local plaintext files. Each record's path is relative to the attachment directory.   |
| `deleteLocal(remote)` | Remove local plaintext, staged ciphertext, and attachment records.                        |

Kotlin uses the calls in the table without argument labels. Swift uses
`pending(remote:)`, `localPath(remote:)`, and `deleteLocal(remote:)`. Browser
and Node expose `remoteAttachment` as a property on the pending handle;
Kotlin and Swift use `remoteAttachment()`.

Pending records survive a restart when the app keeps the same database and
attachment directory. The default maximum pending age is 86,400 seconds.
`AttachmentOptions.maxPendingAgeSeconds` changes it. Expired records and staged
ciphertext are removed at client creation and by the background cleanup worker.
Successful uploads release staged ciphertext and disappear from `listPending`.
The local plaintext remains until the app deletes it.

The SDK does not retry failed transfers automatically. Handle the typed
attachment error and its retryable detail. A failed status also carries an
`AttachmentFailure` with a cause and, when available, an HTTP status. A network
failure can succeed after a delay. Invalid data, an oversized file, or unusable
staged ciphertext needs a corrected input. A credential failure needs the
credential action reported by the error. See the
[failure causes](/specs/atch-remote-attachments/#7-failures) before you retry.

Subscribe to client events to show transfer state. Upload and download events
report started, completed, and failed transfers; `attachment.deleted` reports
local deletion. These are state events, not byte progress updates. Each client
reports only the changes it makes.

Keep the whole client storage directory when you need to resume transfers.
Local deletion does not delete the remote object or its XMTP message. It ends
transfers managed by this client for the deleted attachment. A later download
can fetch the object again while storage still serves it. An unchanged
attachment forwarded in another message shares the same local file.

## How remote attachments work

XMTP messages have a maximum size limit. Files that exceed this limit can't be sent inline and are instead handled as **remote attachments**. For this, the file is encrypted, uploaded to an external storage provider, and a reference URL is sent in the message. The recipient then downloads and decrypts the file using the metadata from the message.

The upload and download limits still apply. SDK-managed uploads use the backend's `maxUploadBytes` setting, which defaults to 100 MiB. Downloads use the app's `maxDownloadBytes` limit, or the backend's upload limit when the app sets none. Without an attachment offer, the default download limit is 100 MiB. A download is also bounded by the advertised `contentLength` and the 32-bit length limit.

For app-owned storage, you need three things:

1. **A file** to attach (image, document, etc.)
2. **A storage provider** to host the encrypted file at an HTTPS URL that answers GET with status 200
3. **An upload callback** that tells the SDK how to upload the encrypted bytes and return a download URL

## Encryption

The SDK encrypts the attachment before the upload callback runs. The host stores an encrypted `EncodedContent` envelope of type `xmtp.org/attachment:1.0`, which contains the file bytes and metadata.

| Property           | Value                                                     |
| ------------------ | --------------------------------------------------------- |
| Cipher             | AES-256-GCM                                               |
| Key derivation     | HKDF-SHA256 with a random 32-byte secret and 32-byte salt |
| Nonce              | Random, 12 bytes                                          |
| Authentication tag | 16 bytes, appended to the ciphertext                      |
| Integrity          | Hex SHA-256 `contentDigest` of the encrypted bytes        |

The upload callback's `attachment.payload` contains encrypted bytes. The download URL and decryption metadata travel in the XMTP message. A storage host that does not have that metadata cannot decrypt the file.

## Structures

### Inline attachment

Type ID: `xmtp.org/attachment:1.0`. Its fallback is `Can't display <filename>. This app doesn't support attachments.`. `shouldPush` defaults to `true` on all four platforms.

| Field      | Type   | Wire location        | Required |
| ---------- | ------ | -------------------- | -------- |
| `filename` | string | `filename` parameter | No       |
| `mimeType` | string | `mimeType` parameter | Yes      |
| `content`  | bytes  | Message content      | Yes      |

### Remote attachment

Type ID: `xmtp.org/remoteStaticAttachment:1.0`. Its fallback names the unsupported file. `shouldPush` defaults to `true` on all four platforms.

| Field           | Type                 | Wire location                                       | Required |
| --------------- | -------------------- | --------------------------------------------------- | -------- |
| `url`           | string               | Message content, UTF-8                              | Yes      |
| `contentDigest` | hex SHA-256          | `contentDigest` parameter                           | Yes      |
| `secret`        | hex string, 32 bytes | `secret` parameter                                  | Yes      |
| `salt`          | hex string, 32 bytes | `salt` parameter                                    | Yes      |
| `nonce`         | hex string, 12 bytes | `nonce` parameter                                   | Yes      |
| `scheme`        | string               | `scheme` parameter                                  | Yes      |
| `contentLength` | integer              | `contentLength` parameter; encrypted payload length | No       |
| `filename`      | string               | `filename` parameter                                | No       |

### Multiple remote attachments

Type ID: `xmtp.org/multiRemoteStaticAttachment:1.0`. The payload is a `MultiRemoteAttachment` protobuf with repeated `RemoteAttachmentInfo` entries. The fallback says that the app does not support multiple remote attachments. `shouldPush` defaults to `true` on all four platforms.

Each `RemoteAttachmentInfo` contains `url`, `contentDigest`, `secret`, `salt`, `nonce`, `scheme`, optional encrypted `contentLength`, and optional `filename`. Each new encrypted file has separate encryption material. The SDK record uses byte arrays for `secret`, `salt`, and `nonce`; the single-attachment codec writes those arrays as hexadecimal parameters.

Each attachment in the attachments array contains a URL that points to an encrypted `EncodedContent` object. The content must be accessible by an HTTP `GET` request to the URL.

## Support multiple remote attachments

Use `MultiRemoteAttachmentCodec` with an `attachments` array of remote attachment records. Each file has its own download and size checks. The message that contains the array must fit the backend's message envelope limit.

## Send and receive with the Agent SDK

Use `ctx.sendRemoteAttachment(file)` to create, encrypt, upload, and send a file through backend attachment storage. No upload callback is required.

To use app-owned storage, supply an upload callback. This example uploads encrypted bytes to Pinata:

```ts source="content-types-attachments-1.ts" region="example1"

```

With the callback in this example:

1. The SDK encodes the attachment envelope and encrypts it
2. Your `uploadCallback` receives the encrypted payload and uploads it to Pinata's IPFS network
3. Pinata returns a CID, which is converted to a gateway URL
4. The SDK sends a message containing the URL and decryption metadata (salt, nonce, secret, content digest)

## Receive and decrypt a remote attachment

When your agent receives an attachment, use `downloadRemoteAttachment` to download and decrypt it in one step:

```ts source="content-types-attachments-2.ts" region="example2"

```

The `downloadRemoteAttachment` utility uses `client.attachments.download()`. Core checks the URL, size, digest, and authentication tag before it returns the local file. The utility reads that file and returns its filename, MIME type, and bytes.

Downloads require HTTPS. Loopback HTTP is permitted only with `allowPrivateNetwork` enabled. Native clients check resolved addresses and redirects. Browser downloads require CORS support and reject redirects. See [Remote attachments specification](/specs/atch-remote-attachments/) for the full transfer rules.

## Support small inline attachments

Use remote attachments for files near the message limit. Inline files share that limit with the content envelope and MLS framing.

The backend's default message envelope limit is 1 MiB. Use the deployment's current `maxEnvelopeBytes` limit. A file smaller than 1 MiB can still exceed the limit after encoding and MLS framing.

To handle unsupported content types, refer to the fallback section.

See [Fallback and unsupported content](/content-types/overview/#fallback-and-unsupported-content).
