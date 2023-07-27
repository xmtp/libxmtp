# Use content types in your app

When you build an app with XMTP, all messages are encoded with a [content type](https://xmtp.org/docs/dev-concepts/content-types) to ensure that an XMTP message API client knows how to encode and decode messages, ensuring interoperability and consistent display of messages across apps.

`xmtp-android` supports the following content types:

- `TextCodec`: This is the default content type and enables sending plain text messages.
- `AttachmentCodec`: Enables sending attachments.
- `RemoteAttachmentCodec`: Enables sending remote attachments.
- `ReactionCodec`: Enables sending of reactions.
- `ReplyCodec`: Enables sending of replies.


## Support remote media attachments

The following examples demonstrate how to use `AttachmentCodec` and `RemoteAttachmentCodec` to support remote attachments in your app. Remote attachments can include rich media such as images, videos, GIFs, and more.

For more details about attachment and remote attachment content types, see [Some new content types](https://xmtp.org/blog/attachments-and-remote-attachments).

### Register your client to accept the codecs

```kotlin
Client.register(codec = AttachmentCodec())
Client.register(codec = RemoteAttachmentCodec())
```

### Create an attachment 

```kotlin
val attachment = Attachment(
filename = "test.txt",
mimeType = "text/plain",
data = "hello world".toByteStringUtf8(),
)
```

### Encode and encrypt an attachment for transport

```kotlin
val encodedEncryptedContent = RemoteAttachment.encodeEncrypted(
    content = attachment,
    codec = AttachmentCodec(),
)
```

### Create a remote attachment from an attachment

```kotlin
val remoteAttachment = RemoteAttachment.from(
    url = URL("https://abcdefg"),
    encryptedEncodedContent = encodedEncryptedContent
)
remoteAttachment.contentLength = attachment.data.size()
remoteAttachment.filename = attachment.filename
```

### Send a remote attachment and set the `contentType`

```kotlin
val newConversation = client.conversations.newConversation(walletAddress)

newConversation.send(
    content = remoteAttachment,
    options = SendOptions(contentType = ContentTypeRemoteAttachment),
)
```

### Receive, decode, and decrypt a remote attachment

```kotlin
val message = newConversation.messages().first()

val loadedRemoteAttachment: RemoteAttachment = messages.content()
loadedRemoteAttachment.fetcher = Fetcher()
runBlocking {
    val attachment: Attachment = loadedRemoteAttachment.load() 
}
```

## Create a custom content type

If you want to send a content type other than plain text, attachments, and remote attachments, you can:

- Propose a new [standard content type](https://github.com/orgs/xmtp/discussions/4) 
- Create a [custom content type](https://xmtp.org/docs/client-sdk/javascript/tutorials/use-content-types#build-a-custom-content-type)
