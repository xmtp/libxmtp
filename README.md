# xmtp-ios

![Test](https://github.com/xmtp/xmtp-ios/actions/workflows/test.yml/badge.svg) ![Lint](https://github.com/xmtp/xmtp-ios/actions/workflows/lint.yml/badge.svg) ![Status](https://img.shields.io/badge/Feature_status-Alpha-orange)

`xmtp-ios` provides a Swift implementation of an XMTP message API client for use with iOS apps.

Use `xmtp-ios` to build with XMTP to send messages between blockchain accounts, including DMs, notifications, announcements, and more.

To keep up with the latest SDK developments, see the [Issues tab](https://github.com/xmtp/xmtp-ios/issues) in this repo.

To learn more about XMTP and get answers to frequently asked questions, see the [XMTP documentation](https://xmtp.org/docs).

![x-red-sm](https://user-images.githubusercontent.com/510695/163488403-1fb37e86-c673-4b48-954e-8460ae4d4b05.png)

## Example app built with `xmtp-ios`

Use the [XMTP iOS quickstart app](https://github.com/xmtp/xmtp-ios/tree/main/example) as a tool to start building an app with XMTP. This basic messaging app has an intentionally unopinionated UI to help make it easier for you to build with.

To learn about example app push notifications, see [Enable the quickstart app to send push notifications](library/src/main/java/org/xmtp/ios/library/push/README.md).

## Reference docs

> **View the reference**
> Access the [Swift client SDK reference documentation](https://xmtp.github.io/xmtp-ios/).

## Install from Swift Package Manager

You can add XMTP-iOS via Swift Package Manager by adding it to your `Package.swift` file or using Xcode’s “Add Package Dependency” feature.

## Usage overview

The XMTP message API revolves around a message API client (client) that allows retrieving and sending messages to other XMTP network participants. A client must connect to a wallet app on startup. If this is the very first time the client is created, the client will generate an identity with an encrypted local database to store and retrieve messages. Each additional log in will create a new installation if a local database is not present.

```swift
// You'll want to replace this with a wallet from your application.
let account = PrivateKey()

// A key to encrypt the local database
let encryptionKey = SymmetricKey(size: .bits256)

// Application context for creating the local database
let context = UIApplication.shared.delegate

// The required client options
let clientOptions = ClientOptions(
    api: .init(env: .dev, isSecure: true),
    dbEncryptionKey: encryptionKey,
    appContext: context
)

// Create the client with your wallet. This will connect to the XMTP `dev` network by default.
// The account is anything that conforms to the `XMTP.SigningKey` protocol.
let client = try Client().create(account: account, options: clientOptions)

// Start a dm conversation
let conversation = try client.conversations.newConversation(with: "0x3F11b27F323b62B159D2642964fa27C46C841897")
// Or a group conversation
let groupConversation = try client.conversations.newGroup(with: ["0x3F11b27F323b62B159D2642964fa27C46C841897"])

// Load all messages in the conversations
let messages = try await conversation.messages()

// Send a message
try await conversation.send("gm")

// Listen for new messages in the conversation
Task {
    for await message in try await conversation.streamMessages() {
        print("\(message.senderInboxId): \(message.body)")
    }
}
```


## Create a client

A client is created with `Client().create(account: SigningKey, options: ClientOptions): Client` that requires passing in an object capable of creating signatures on your behalf. The client will request a signature for any new installation.

> **Note**
> The client connects to the XMTP `dev` environment by default. [Use `ClientOptions`](#configure-the-client) to change this and other parameters of the network connection.

```swift

// Create the client with a `SigningKey` from your app
let options = ClientOptions(api: ClientOptions.Api(env: .production, isSecure: true), dbEncryptionKey: encryptionKey, appContext: context)
let client = try Client().create(account: account, options: options)
```
### Create a client from saved encryptionKey

You can save your encryptionKey for the local database and build the client via address:

```swift
// Create the client with a `SigningKey` from your app
let options = ClientOptions(api: ClientOptions.Api(env: .production, isSecure: true), dbEncryptionKey: encryptionKey, appContext: context)
let client = try Client().build(address: account.address, options: options)
``
### Configure the client

You can configure the client with these parameters of `Client.create`:

| Parameter  | Default     | Description                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ---------- |-------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| env        | `DEV`       | Connect to the specified XMTP network environment. Valid values include `DEV`, `.PRODUCTION`, or `LOCAL`. For important details about working with these environments, see [XMTP `production` and `dev` network environments](#xmtp-production-and-dev-network-environments).                                                                                                                                                            |
| appContext | `REQUIRED`  | The application context used to create and access the local database.                                                                                                                                                                                                                                                                                                                                                                    |
| dbEncryptionKey | `REQUIRED`  | A 32 ByteArray used to encrypt the local database.                                                                                                                                                                                                                                                                                                                                                                                       |
| historySyncUrl | `https://message-history.dev.ephemera.network/` | The history sync url used to specify where history can be synced from other devices on the network.                                                                                                                                                                                                                                                                                             

**Configure `env`**

## Handle conversations

Most of the time, when interacting with the network, you'll want to do it through `conversations`. Conversations are between two accounts.

### List all dm & group conversations

If your app would like to handle groups and dms differently you can check whether a conversation is a dm or group for the type
```swift
let conversations = try await client.conversations.list()

for conversation in conversations {
    switch conversation.type {
    case .group:
        // Handle group
    case .dm:
        // Handle DM
    }
}
```
### List all groups
```swift
let groups = try await client.conversations.listGroups()
```
### List all DMs
```swift
let dms = try await client.conversations.listDms()
```
These conversations include all conversations for a user **regardless of which app created the conversation.** This functionality provides the concept of an [interoperable inbox](https://xmtp.org/docs/concepts/interoperable-inbox), which enables a user to access all of their conversations in any app built with XMTP.

### Listen for new conversations

You can also listen for new conversations being started in real-time. This will allow apps to display incoming messages from new contacts.

```swift
Task {
    for await conversation in try await client.conversations.stream() {
        print("New conversation started with \(conversation.peerAddress)")
        // Say hello to your new friend
        try await conversation.send("Hi there!")
    }
}
```
### Start a new conversation

You can create a new conversation with any Ethereum address on the XMTP network.

```swift
let newDm = try client.conversations.newConversation(with: "0x3F11b27F323b62B159D2642964fa27C46C841897")
```
```swift
let newGroup = try client.conversations.newGroup(with: ["0x3F11b27F323b62B159D2642964fa27C46C841897"])
```
### Send messages

To be able to send a message, the recipient must have already created a client at least once. Messages are addressed using account addresses. In this example, the message payload is a plain text string.

```swift

let conversation = try client.conversations.newConversation(with: "0x3F11b27F323b62B159D2642964fa27C46C841897")
try await conversation.send("Hello world")
```
To learn how to send other types of content, see [Handle different content types](#handle-different-types-of-content).

### List messages in a conversation

You can receive the complete message history in a conversation by calling `conversation.messages()`

```swift

let messages = try await conversation.messages()
``
### List messages in a conversation with pagination

It may be helpful to retrieve and process the messages in a conversation page by page. You can do this by calling `conversation.messages(limit: Int, before: Date)` which will return the specified number of messages sent before that time.


```swift

let messages = try await conversation.messages(limit: 25)
let nextPage = try await conversation.messages(limit: 25, before: messages.first?.sentDate)
```
### Listen for new messages in a conversation

You can listen for any new messages (incoming or outgoing) in a conversation by calling `conversation.streamMessages()`.

A successfully received message (that makes it through the decoding and decryption without throwing) can be trusted to be authentic. Authentic means that it was sent by the owner of the `message.senderInboxId` account and that it wasn't modified in transit. The `message.sent` timestamp can be trusted to have been set by the sender.

The flow returned by the `stream` methods is an asynchronous data stream that sequentially emits values and completes normally or with an exception.


```swift

Task {
    for await message in try await conversation.streamMessages() {
        if message.senderInboxId == client.address {
            // This message was sent from me
        }
        print("New message from \(message.senderInboxId): \(message.body)")
    }
}
```
## Request and respect user consent

![Feature status](https://img.shields.io/badge/Feature_status-Alpha-orange)

The user consent feature enables your app to request and respect user consent preferences. With this feature, another blockchain account address registered on the XMTP network can have one of three consent preference values:

- Unknown
- Allowed
- Denied

To learn more, see [Request and respect user consent](https://xmtp.org/docs/build/user-consent).

## Handle different types of content

All the send functions support `SendOptions` as an optional parameter. The `contentType` option allows specifying different types of content than the default simple string, which is identified with content type identifier `ContentTypeText`.

To learn more about content types, see [Content types with XMTP](https://xmtp.org/docs/concepts/content-types).

Support for other types of content can be added by registering additional `ContentCodec`s with the Client. Every codec is associated with a content type identifier, `ContentTypeId`, which is used to signal to the Client which codec should be used to process the content that is being sent or received.

```swift

// Assuming we've loaded a fictional NumberCodec that can be used to encode numbers,
// and is identified with ContentTypeNumber, we can use it as follows.
Client.register(codec: NumberCodec())

let options = SendOptions(contentType: .number, contentFallback: "sending you a pie")
try await aliceConversation.send(3.14, options: options)
```
As shown in the example above, you must provide a `contentFallback` value. Use it to provide an alt text-like description of the original content. Providing a `contentFallback` value enables clients that don't support the content type to still display something meaningful.

> **Caution**
> If you don't provide a `contentFallback` value, clients that don't support the content type will display an empty message. This results in a poor user experience and breaks interoperability.

### Handle custom content types

Beyond this, custom codecs and content types may be proposed as interoperable standards through XRCs. To learn more about the custom content type proposal process, see [XIP-5](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-5-message-content-types.md).

## 🏗 Breaking revisions

Because `xmtp-android` is in active development, you should expect breaking revisions that might require you to adopt the latest SDK release to enable your app to continue working as expected.

XMTP communicates about breaking revisions in the [XMTP Discord community](https://discord.gg/xmtp), providing as much advance notice as possible. Additionally, breaking revisions in an `xmtp-android` release are described on the [Releases page](https://github.com/xmtp/xmtp-android/releases).

## Deprecation

Older versions of the SDK will eventually be deprecated, which means:

1. The network will not support and eventually actively reject connections from clients using deprecated versions.
2. Bugs will not be fixed in deprecated versions.

The following table provides the deprecation schedule.

| Announced              | Effective     | Minimum Version | Rationale                                                                                                                                                                  |
|------------------------|---------------|-----------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| No more support for V2 | March 1, 2025 | 3.0.0           | In a move towards better security with MLS and the ability to decentralize we will be shutting down V2 and moving entirely to V3 MLS. You can see the legacy branch here: https://github.com/xmtp/xmtp-android/tree/xmtp-legacy |

Bug reports, feature requests, and PRs are welcome in accordance with these [contribution guidelines](https://github.com/xmtp/xmtp-android/blob/main/CONTRIBUTING.md).

## XMTP `production` and `dev` network environments

XMTP provides both `production` and `dev` network environments to support the development phases of your project.

The `production` and `dev` networks are completely separate and not interchangeable.
For example, for a given blockchain account, its XMTP identity on `dev` network is completely distinct from its XMTP identity on the `production` network, as are the messages associated with these identities. In addition, XMTP identities and messages created on the `dev` network can't be accessed from or moved to the `production` network, and vice versa.

> **Note**
> When you [create a client](#create-a-client), it connects to the XMTP `dev` environment by default. To learn how to use the `env` parameter to set your client's network environment, see [Configure the client](#configure-the-client).

The `env` parameter accepts one of three valid values: `dev`, `production`, or `local`. Here are some best practices for when to use each environment:

- `dev`: Use to have a client communicate with the `dev` network. As a best practice, set `env` to `dev` while developing and testing your app. Follow this best practice to isolate test messages to `dev` inboxes.

- `production`: Use to have a client communicate with the `production` network. As a best practice, set `env` to `production` when your app is serving real users. Follow this best practice to isolate messages between real-world users to `production` inboxes.

- `local`: Use to have a client communicate with an XMTP node you are running locally. For example, an XMTP node developer can set `env` to `local` to generate client traffic to test a node running locally.

The `production` network is configured to store messages indefinitely. XMTP may occasionally delete messages and keys from the `dev` network, and will provide advance notice in the [XMTP Discord community](https://discord.gg/xmtp).
