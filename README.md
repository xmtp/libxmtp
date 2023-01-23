# XMTP-iOS

![Test](https://github.com/xmtp/xmtp-ios/actions/workflows/test.yml/badge.svg) ![Lint](https://github.com/xmtp/xmtp-ios/actions/workflows/lint.yml/badge.svg) ![Status](https://img.shields.io/badge/Project_Status-Developer_Preview-yellow)

`xmtp-ios` provides a Swift implementation of an XMTP message API client for use with iOS apps.

Use `xmtp-ios` to build with XMTP to send messages between blockchain accounts, including DMs, notifications, announcements, and more.

This SDK is in **Developer Preview** status and ready for you to start building.

However, we do **not** recommend using Developer Preview software in production apps. Software in this status may change based on feedback.

Specifically, this SDK is missing this functionality:

- Specifying `apiUrl`, `keyStoreType`, `codecs`, `maxContentSize`, and `appVersion` when creating a `Client`
- Streaming all messages from all conversations

Follow along in the [tracking issue](https://github.com/xmtp/xmtp-ios/issues/7) for updates.

To learn more about XMTP and get answers to frequently asked questions, see [FAQ about XMTP](https://xmtp.org/docs/dev-concepts/faq).

![x-red-sm](https://user-images.githubusercontent.com/510695/163488403-1fb37e86-c673-4b48-954e-8460ae4d4b05.png)

## Example app

For a basic demonstration of the core concepts and capabilities of the `xmtp-ios` client SDK, see the [Example app project](https://github.com/xmtp/xmtp-ios/tree/main/XMTPiOSExample/XMTPiOSExample).


## Install with Swift Package Manager

Use Xcode to add to the project (**File** > **Add Packages…**) or add this to your `Package.swift` file:

```swift
.package(url: "https://github.com/xmtp/xmtp-ios", branch: "main")
```

## Usage overview

The XMTP message API revolves around a message API client (client) that allows retrieving and sending messages to other XMTP network participants. A client must connect to a wallet app on startup. If this is the very first time the client is created, the client will generate a key bundle that is used to encrypt and authenticate messages. The key bundle persists encrypted in the network using an account signature. The public side of the key bundle is also regularly advertised on the network to allow parties to establish shared encryption keys. All of this happens transparently, without requiring any additional code.

```swift
import XMTP

// You'll want to replace this with a wallet from your application.
let account = try PrivateKey.generate()

// Create the client with your wallet. This will connect to the XMTP `dev` network by default.
// The account is anything that conforms to the `XMTP.SigningKey` protocol.
let client = try await Client.create(account: account)

// Start a conversation with XMTP
let conversation = try await client.conversations.newConversation(with: "0x3F11b27F323b62B159D2642964fa27C46C841897")

// Load all messages in the conversation
let messages = try await conversation.messages()
// Send a message
try await conversation.send(content: "gm")
// Listen for new messages in the conversation
for try await message in conversation.streamMessages() {
  print("\(message.senderAddress): \(message.body)")
}
```

## Create a client

A client is created with `Client.create(account: SigningKey) async throws -> Client` that requires passing in an object capable of creating signatures on your behalf. The client will request a signature in two cases:

1. To sign the newly generated key bundle. This happens only the very first time when a key bundle is not found in storage.
2. To sign a random salt used to encrypt the key bundle in storage. This happens every time the client is started, including the very first time).

**Important:** The client connects to the XMTP `dev` environment by default. [Use `ClientOptions`](#configuring-the-client) to change this and other parameters of the network connection.

```swift
import XMTP

// Create the client with a `SigningKey` from your app
let client = try await Client.create(account: account, options: .init(api: .init(env: .production)))
```

### Creating a client from saved keys

You can save your keys from the client via the `privateKeyBundle` property:

```swift
// Create the client with a `SigningKey` from your app
let client = try await Client.create(account: account, options: .init(api: .init(env: .production)))

// Get the key bundle
let keys = client.privateKeyBundle

// Serialize the key bundle and store it somewhere safe
let keysData = try keys.serializedData()
```

Once you have those keys, you can create a new client with `Client.from`:

```swift
let keys = try PrivateKeyBundle(serializedData: keysData)
let client = try Client.from(bundle: keys, options: .init(api: .init(env: .production)))
```

### Configure the client

You can configure the client's network connection and key storage method with these optional parameters of `Client.create`:

| Parameter | Default | Description                                                                                                                                                                                                                                                                     |
| --------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| env       | `dev`   | Connect to the specified XMTP network environment. Valid values include `.dev`, `.production`, or `.local`. For important details about working with these environments, see [XMTP `production` and `dev` network environments](#xmtp-production-and-dev-network-environments). |

```swift
// Configure the client to use the `production` network
let clientOptions = ClientOptions(api: .init(env: .production))
let client = try await Client.create(account: account, options: clientOptions)
```

**Note: The `apiUrl`, `keyStoreType`, `codecs`, `maxContentSize` and `appVersion` parameters from the XMTP client SDK for JavaScript (xmtp-js) are not yet supported.**

## Handle conversations

Most of the time, when interacting with the network, you'll want to do it through `conversations`. Conversations are between two accounts.

```swift
import XMTP
// Create the client with a wallet from your app
let client = try await Client.create(account: account)
let conversations = try await client.conversations.list()
```

### List existing conversations

You can get a list of all conversations that have had one or more messages exchanged in the last 30 days.

```swift
let allConversations = try await client.conversations.list()

for conversation in allConversations {
  print("Saying GM to \(conversation.peerAddress)")
  try await conversation.send(content: "gm")
}
```

### Listen for new conversations

You can also listen for new conversations being started in real-time. This will allow apps to display incoming messages from new contacts.

**Warning: This stream will continue infinitely. To end the stream, break from the loop.**

```swift
for try await conversation in client.conversations.stream() {
  print("New conversation started with \(conversation.peerAddress)")

  // Say hello to your new friend
  try await conversation.send(content: "Hi there!")

  // Break from the loop to stop listening
  break
}
```

### Start a new conversation

You can create a new conversation with any Ethereum address on the XMTP network.

```swift
let newConversation = try await client.conversations.newConversation(with: "0x3F11b27F323b62B159D2642964fa27C46C841897")
```

### Send messages

To be able to send a message, the recipient must have already created a client at least once and consequently advertised their key bundle on the network. Messages are addressed using account addresses. The message payload must be a plain string.

**Note: Other types of content are currently not supported.**

```swift
let conversation = try await client.conversations.newConversation(with: "0x3F11b27F323b62B159D2642964fa27C46C841897")
try await conversation.send(content: "Hello world")
```

### List messages in a conversation

You can receive the complete message history in a conversation by calling `conversation.messages()`

```swift
for conversation in client.conversations.list() {
  let messagesInConversation = try await conversation.messages()
}
```

### List messages in a conversation with pagination

It may be helpful to retrieve and process the messages in a conversation page by page. You can do this by calling `conversation.messages(limit: Int, before: Date)` which will return the specified number of messages sent before that time.

```swift
let conversation = try await client.conversations.newConversation(with: "0x3F11b27F323b62B159D2642964fa27C46C841897")

let messages = try await conversation.messages(limit: 25)
let nextPage = try await conversation.messages(limit: 25, before: messages[0].sent)
```

### Listen for new messages in a conversation

You can listen for any new messages (incoming or outgoing) in a conversation by calling `conversation.streamMessages()`.

A successfully received message (that makes it through the decoding and decryption without throwing) can be trusted to be authentic. Authentic means that it was sent by the owner of the `message.senderAddress` account and that it wasn't modified in transit. The `message.sent` timestamp can be trusted to have been set by the sender.

The stream returned by the `stream` methods is an asynchronous iterator and as such is usable by a for-await-of loop. Note however that it is by its nature infinite, so any looping construct used with it will not terminate, unless the termination is explicitly initiated (by breaking the loop).

```swift
let conversation = try await client.conversations.newConversation(with: "0x3F11b27F323b62B159D2642964fa27C46C841897")

for try await message in conversation.streamMessages() {
  if message.senderAddress == client.address {
    // This message was sent from me
    continue
  }

  print("New message from \(message.senderAddress): \(message.body)")
}
```

**Note: This package does not currently include the `streamAllMessages()` functionality from the XMTP client SDK for JavaScript (xmtp-js).**

### Handling multiple conversations with the same blockchain address

With XMTP, you can have multiple ongoing conversations with the same blockchain address. For example, you might want to have a conversation scoped to your particular app, or even a conversation scoped to a particular item in your app.

To accomplish this, you can pass a context with a `conversationId` when you are creating a conversation. We recommend conversation IDs start with a domain, to help avoid unwanted collisions between your app and other apps on the XMTP network.

```swift
// Start a scoped conversation with ID mydomain.xyz/foo
let conversation1 = try await client.conversations.newConversation(
  with: "0x3F11b27F323b62B159D2642964fa27C46C841897",
  context: .init(conversationID: "mydomain.xyz/foo")
)

// Start a scoped conversation with ID mydomain.xyz/bar. And add some metadata
let conversation2 = try await client.conversations.newConversation(
  with: "0x3F11b27F323b62B159D2642964fa27C46C841897",
  context: .init(conversationID: "mydomain.xyz/bar", metadata: ["title": "Bar conversation"])
)

// Get all the conversations
let conversations = try await client.conversations.list()

// Filter for the ones from your app
let myAppConversations = conversations.filter {
  guard let conversationID = $0.context?.conversationID else {
    return false
  }

  return conversationID.hasPrefix("mydomain.xyz/")
}
```

### Decoding a single message

You can decode a single `Envelope` from XMTP using the `decode` method:

```swift
let conversation = try await client.conversations.newConversation(with: "0x3F11b27F323b62B159D2642964fa27C46C841897")

// Assume this function returns an Envelope that contains a message for the above conversation
let envelope = getEnvelopeFromXMTP()

let decodedMessage = try conversation.decode(envelope)
```

### Serialize/Deserialize conversations

You can save a conversation object locally using its `encodedContainer` property. This returns a `ConversationContainer` object which conforms to `Codable`.

```swift
// Get a conversation
let conversation = try await client.conversations.newConversation(with: "0x3F11b27F323b62B159D2642964fa27C46C841897")

// Get a container.
let container = conversation.encodedContainer

// Dump it to JSON
let encoder = JSONEncoder()
let data = try encoder.encode(container)

// Get it back from JSON
let decoder = JSONDecoder()
let containerAgain = try decoder.decode(ConversationContainer.self, from: data)

// Get an actual Conversation object like we had above
let decodedConversation = containerAgain.decode(with: client)
try await decodedConversation.send(text: "hi")
```

### Different types of content

All the send functions support SendOptions as an optional parameter. The contentType option allows specifying different types of content than the default simple string, which is identified with content type identifier ContentTypeText. Support for other types of content can be added by registering additional ContentCodecs with the Client. Every codec is associated with a content type identifier, ContentTypeId, which is used to signal to the Client which codec should be used to process the content that is being sent or received. See XIP-5 for more details on codecs and content types.

Codecs and content types may be proposed as interoperable standards through XRCs. If there is a concern that the recipient may not be able to handle a non-standard content type, the sender can use the contentFallback option to provide a string that describes the content being sent. If the recipient fails to decode the original content, the fallback will replace it and can be used to inform the recipient what the original content was.

```swift
// Assuming we've loaded a fictional NumberCodec that can be used to encode numbers,
// and is identified with ContentTypeNumber, we can use it as follows.
Client.register(codec: NumberCodec())

try await aliceConversation.send(content: 3.14, options: .init(contentType: ContentTypeNumber, contentFallback: "sending you a pie"))
```

### Compression

Message content can be optionally compressed using the compression option. The value of the option is the name of the compression algorithm to use. Currently supported are gzip and deflate. Compression is applied to the bytes produced by the content codec.

Content will be decompressed transparently on the receiving end. Note that Client enforces maximum content size. The default limit can be overridden through the ClientOptions. Consequently a message that would expand beyond that limit on the receiving end will fail to decode.

```swift
try await conversation.send(text: '#'.repeat(1000), options: .init(compression: .gzip))
```

## 🏗 **Breaking revisions**

Because `xmtp-ios` is in active development, you should expect breaking revisions that might require you to adopt the latest SDK release to enable your app to continue working as expected.

XMTP communicates about breaking revisions in the [XMTP Discord community](https://discord.gg/xmtp), providing as much advance notice as possible. Additionally, breaking revisions in an `xmtp-ios` release are described on the [Releases page](https://github.com/xmtp/xmtp-ios/releases).

## Deprecation

Older versions of the SDK will eventually be deprecated, which means:

1. The network will not support and eventually actively reject connections from clients using deprecated versions.
2. Bugs will not be fixed in deprecated versions.

The following table provides the deprecation schedule.

| Announced  | Effective  | Minimum Version | Rationale                                                                                                         |
| ---------- | ---------- | --------------- | ----------------------------------------------------------------------------------------------------------------- |
| There are no deprecations scheduled for `xmtp-ios` at this time. |  |          |  |

Bug reports, feature requests, and PRs are welcome in accordance with these [contribution guidelines](https://github.com/xmtp/xmtp-ios/blob/main/CONTRIBUTING.md).

## XMTP `production` and `dev` network environments

XMTP provides both `production` and `dev` network environments to support the development phases of your project.

The `production` and `dev` networks are completely separate and not interchangeable.
For example, for a given blockchain account, its XMTP identity on `dev` network is completely distinct from its XMTP identity on the `production` network, as are the messages associated with these identities. In addition, XMTP identities and messages created on the `dev` network can't be accessed from or moved to the `production` network, and vice versa.

**Important:** When you [create a client](#create-a-client), it connects to the XMTP `dev` environment by default. To learn how to use the `env` parameter to set your client's network environment, see [Configure the client](#configure-the-client).

The `env` parameter accepts one of three valid values: `dev`, `production`, or `local`. Here are some best practices for when to use each environment:

- `dev`: Use to have a client communicate with the `dev` network. As a best practice, set `env` to `dev` while developing and testing your app. Follow this best practice to isolate test messages to `dev` inboxes.

- `production`: Use to have a client communicate with the `production` network. As a best practice, set `env` to `production` when your app is serving real users. Follow this best practice to isolate messages between real-world users to `production` inboxes.

- `local`: Use to have a client communicate with an XMTP node you are running locally. For example, an XMTP node developer can set `env` to `local` to generate client traffic to test a node running locally.

The `production` network is configured to store messages indefinitely. XMTP may occasionally delete messages and keys from the `dev` network, and will provide advance notice in the [XMTP Discord community](https://discord.gg/xmtp).
