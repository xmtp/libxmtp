---
title: Support attachments in your app built with XMTP
---

Use the remote attachment, multiple remote attachments, or attachment content type to support attachments in your app.

- Use the [remote attachment content type](#support-remote-attachments-of-any-size) to send one remote attachment of any size.
- Use the [multiple remote attachments content type](#support-multiple-remote-attachments-of-any-size) to send multiple remote attachments of any size.
- Use the [attachment content type](#support-attachments-smaller-than-1mb) to send attachments smaller than 1MB.

## Support remote attachments of any size

One remote attachment of any size can be sent in a message using the `RemoteAttachmentCodec` and a storage provider.

To send multiple remote attachments of any size in a single message, see [Support multiple remote attachments of any size](#support-multiple-remote-attachments-of-any-size).

## How remote attachments work

XMTP messages have a maximum size limit. Files that exceed this limit can't be sent inline and are instead handled as **remote attachments**. For this, the file is encrypted, uploaded to an external storage provider, and a reference URL is sent in the message. The recipient then downloads and decrypts the file using the metadata from the message.

This approach keeps messages lightweight while supporting files of any size. To send an attachment, you need three things:

1. **A file** to attach (image, document, etc.)
2. **A storage provider** to host the encrypted file (any service that supports HTTPS GET requests)
3. **An upload callback** that tells the SDK how to upload the encrypted bytes and return a download URL

## Encryption

The SDK encrypts the attachment before the upload callback runs. The host stores an encoded `Attachment`, not the raw file.

| Property           | Value                                                     |
| ------------------ | --------------------------------------------------------- |
| Cipher             | AES-256-GCM                                               |
| Key derivation     | HKDF-SHA256 with a random 32-byte secret and 32-byte salt |
| Nonce              | Random, 12 bytes                                          |
| Authentication tag | 16 bytes, appended to the ciphertext                      |
| Integrity          | Hex SHA-256 `contentDigest` of the encrypted bytes        |

The `attachment.payload` contains the already-encrypted bytes, so what gets stored on IPFS is unreadable without the decryption keys, which are only shared within the XMTP message.

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

| Field           | Type                 | Wire location                                       | Required          |
| --------------- | -------------------- | --------------------------------------------------- | ----------------- |
| `url`           | string               | Message content, UTF-8                              | Yes               |
| `contentDigest` | hex SHA-256          | `contentDigest` parameter                           | Yes               |
| `secret`        | hex string, 32 bytes | `secret` parameter                                  | Yes               |
| `salt`          | hex string, 32 bytes | `salt` parameter                                    | Yes               |
| `nonce`         | hex string, 12 bytes | `nonce` parameter                                   | Yes               |
| `scheme`        | string               | `scheme` parameter                                  | Yes when encoding |
| `contentLength` | integer              | `contentLength` parameter; encrypted payload length | No                |
| `filename`      | string               | `filename` parameter                                | No                |

### Multiple remote attachments

Type ID: `xmtp.org/multiRemoteStaticAttachment:1.0`. The payload is a `MultiRemoteAttachment` protobuf with repeated `RemoteAttachmentInfo` entries. The fallback says that the app does not support multiple remote attachments. `shouldPush` defaults to `true` on all four platforms.

Each `RemoteAttachmentInfo` contains `url`, `contentDigest`, `secret`, `salt`, `nonce`, `scheme`, optional encrypted `contentLength`, and optional `filename`. Each entry has separate encryption material.

Each attachment in the attachments array contains a URL that points to an encrypted `EncodedContent` object. The content must be accessible by an HTTP `GET` request to the URL.

## Support multiple remote attachments of any size

Multiple remote attachments of any size can be sent in a single message using the `MultiRemoteAttachmentCodec` and a storage provider.

## Send and receive

Use `ctx.sendRemoteAttachment` to send a file as an encrypted remote attachment. You provide the file and an upload callback that handles storing the encrypted bytes:

```ts source="content-types-attachments-1.ts" region="example1"

```

Here's what happens under the hood when you call `sendRemoteAttachment`:

1. The SDK encrypts the file contents
2. Your `uploadCallback` receives the encrypted payload and uploads it to Pinata's IPFS network
3. Pinata returns a CID, which is converted to a gateway URL
4. The SDK sends a message containing the URL and decryption metadata (salt, nonce, secret, content digest)

## Receive and decrypt a remote attachment

When your agent receives an attachment, use `downloadRemoteAttachment` to download and decrypt it in one step:

```ts source="content-types-attachments-2.ts" region="example2"

```

The `downloadRemoteAttachment` utility handles fetching the encrypted bytes from the remote URL and decrypting them using the metadata from the message. You get back the original file with its filename, MIME type, and data.

## Support attachments smaller than 1MB

:::caution
Unless a very specific use case we recommend using the [remote attachment content type](/content-types/attachments/) instead since many attachments are larger than 1MB in a chat app.
:::

An encoded envelope is capped at 1 MiB. MLS framing uses part of this limit. Use a remote attachment for a file near the cap.

To handle unsupported content types, refer to the fallback section.

See [Fallback and unsupported content](/content-types/overview/#fallback-and-unsupported-content).
