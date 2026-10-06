# XMTP client SDK for Node

This package provides the XMTP client SDK for Node.

To keep up with the latest SDK developments, see the [Issues tab](https://github.com/xmtp/xmtp-js/issues) in this repo.

## Documentation

To learn how to use the XMTP client SDK for Node, see [Get started with the XMTP Node SDK](https://docs.xmtp.org/sdks/node).

## Backend configuration

Pass a backend and a storage location to `Client.create` or `Client.build`.
The backend URL must include `http://` or `https://`.

```typescript
const client = await Client.create(signer, {
  backend: { url: "http://127.0.0.1:5050", appVersion: "my-app/1.0.0" },
  storage: { location: "default", label: "local" },
});
```

Messages have tagged content. For a text message, read `message.content.value`
after checking `message.content.kind === "text"`. Custom codecs implement
`ContentCodec<T>` and expose their `ContentTypeId` in the `type` field.

## Requirements

- Node.js 22.12 or later.
- ESM imports.

## Install

### NPM

```bash
npm install @xmtp/node-sdk
```

### PNPM

```bash
pnpm install @xmtp/node-sdk
```

### Yarn

```bash
yarn add @xmtp/node-sdk
```

## Developing

For repository development, run `dev/nix-shell 'just install-js'` once. Generate the Node product with
`dev/nix-shell 'just sdk generate node'`, then build with
`NIX_DEVSHELL=js-node dev/nix-shell 'pnpm --filter @xmtp/node-sdk build'`.
The release package is `target/sdk-packages/node`. The source package is a
development shell. Do not publish the source directory.

## Version 8 migration

Use `Client.create(signer, options)` to register a client. Use
`Client.build(identity, options)` to open stored client state. Set the network
with `options.backend.url`. Set the database location with `options.storage`.
The signer implements `identity()`, `kind()` and `sign(request)`.

Read a group or DM state with `await conversation.state()`. Message times use
`Timestamp`. Message counts are `bigint`. Messages contain tagged content;
check `message.content.kind` before you read its `value`.
Configuration `uint64` fields are `bigint`. The four frame-rate and burst
fields remain `number`.

Use `client.conversations.stream()` or `client.conversations.streamAllMessages()` and
await `ready()`. Use `onValue()` for callbacks or `for await` for iteration.
A failed stream closes once and reports `onClose({ kind: "failed", error })`.
Later `next()` calls reject with the same terminal error.
Open an explicit replacement when the app is ready. End streams and clients
with `await stream.end()` and `await client.end()`.

Without `selection.from`, only one default message reader can own delivery
progress for a client database. Another default reader fails with
`ConsumerOwned`, even for a different group, DM, or filter. End the current
reader before opening another default reader.

An explicit `selection.from` cursor opens an independent replay/live reader.
These readers can run in parallel and do not change default delivery progress.
They do not have separate durable consumer checkpoints. To resume a replay,
save the last processed message's `deliveryCursor` and pass it as `from` when
opening the next reader. The cursor must come from the same database.

The next read acknowledges the prior message. In a `for await` loop, await all
message processing before the next iteration. With `onValue()`, await all
processing in the callback. Adding a message to an app queue or starting an
unawaited task does not wait for that work before acknowledgement. `end()` and
`return()` do not acknowledge the last message. They cannot undo an
acknowledgement after its commit has been admitted.

Import `ContentCodec<T>`, `EncodedContent` and `ContentTypeId` from this SDK.
A codec has a `type` field. Encoded parameters use `Map<string, string>`.
Stored-message content filters accept the supported built-in `ContentTypeId`
values. The old `ContentType.Custom` wildcard category is removed. Custom
content can still be sent, decoded and selected by an exact event filter.

Use `client.attachments` for transfer operations and `client.archives` for
archive operations. For application-hosted files, use the public Rust envelope
and attachment encryption helpers.

## Testing

Run tests from the repository root inside Nix:

```bash
dev/nix-shell 'just js test-node-sdk-ci'
dev/nix-shell 'just js check'
```

## Breaking revisions

Because this SDK is in active development, you should expect breaking revisions that might require you to adopt the latest SDK release to enable your app to continue working as expected.

Breaking revisions in a Node SDK release are described on the [Releases page](https://github.com/xmtp/xmtp-js/releases).

## Deprecation

Older versions of the SDK will eventually be deprecated, which means:

1. The network will not support and eventually actively reject connections from clients using deprecated versions.
2. Bugs will not be fixed in deprecated versions.

The following table provides the deprecation schedule.

| Announced                   | Effective   | Minimum Version | Rationale                                                                                                                                                                                                                                                                                                                                                                                                                |
| --------------------------- | ----------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| No more support for XMTP V2 | May 1, 2025 | >=1.0.5         | In a move toward better security with MLS and the ability to decentralize, we will be shutting down XMTP V2 and moving entirely to XMTP V3. To learn more about V2 deprecation, see [XIP-53: XMTP V2 deprecation plan](https://community.xmtp.org/t/xip-53-xmtp-v2-deprecation-plan/867). To learn how to upgrade, see [@xmtp/node-sdk v1.0.5](https://github.com/xmtp/xmtp-js/releases/tag/%40xmtp%2Fnode-sdk%401.0.5). |

Bug reports, feature requests, and PRs are welcome in accordance with these [contribution guidelines](https://github.com/xmtp/xmtp-js/blob/main/CONTRIBUTING.md).
