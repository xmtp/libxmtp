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

## Send and receive

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
