<!-- markdownlint-configure-file {"MD041": false, "MD033": {"allowed_elements": ["p", "img"]}} -->

<p align="center">
  <img src="https://raw.githubusercontent.com/xmtp/brand/1bf5822708c9ce7e06964b85121093d69b3a4ff2/assets/postmark-outlined-color.svg" alt="XMTP" width="128">
</p>

# XMTP

**Build secure messaging into your app.**

[![CI](https://github.com/xmtp/libxmtp/actions/workflows/ci.yml/badge.svg)](https://github.com/xmtp/libxmtp/actions/workflows/ci.yml)
[![Version 8 RC](https://img.shields.io/badge/SDKs-8.0.0--rc2-blue)](https://xmtp.org/get-started/install/)
[![MIT license](https://img.shields.io/badge/License-MIT-blue)](LICENSE)

XMTP is an open protocol for end-to-end encrypted direct messages and group conversations. Use it to add chat to your mobile or web app, connect people with agents from other apps, or connect agents to agents.

This repository contains the shared Rust implementation, a ready to deploy messaging backend, and SDKs for browsers, Node.js, Android, iOS, and agents. The SDKs handle message encryption, identity, group state, and local storage so you can focus on your app.

[Documentation](https://xmtp.org/) · [SDK source](https://github.com/xmtp/libxmtp/tree/main/sdks) · [Example apps](https://github.com/xmtp/libxmtp/tree/main/apps) · [Contributing](https://github.com/xmtp/libxmtp/blob/main/CONTRIBUTING.md)

## Why use XMTP

- **Security built on MLS.** Messaging Layer Security provides end-to-end encryption, forward secrecy, and post-compromise security. XMTP provides protection against harvest now, decrypt later (HNDL) attacks, where messages are stored today for a future quantum computer to try to decrypt. SQLCipher is officially supported for encrypting native client databases at rest. See the [security model](https://xmtp.org/protocol/security/).

- **Private groups and profiles.** Conversation members can manage membership and permissions without the server operator seeing either. Keep group metadata and member profiles encrypted, with configurable fields and permissions. See the [group guide](https://xmtp.org/sdk/groups/).

- **Private keys are first class identities.** Create an account with a cryptographic private key. The identity registry provides an immutable ledger of all keys added to or removed from accounts. Clients verify signed updates, and your app keeps the private key. See the [identity model](https://xmtp.org/protocol/identity/).

- **Rich messages.** Use standard content types for replies, reactions, and attachments. Add custom content types for your app. XMTP supports remote attachments for encrypted files. See the [content type guide](https://xmtp.org/content-types/overview/).

- **One stack across platforms.** Build with Browser or Node SDKs and Android and iOS native SDKs. XMTP includes a ready to deploy messaging backend.

XMTP powers messaging in products from [Coinbase](https://help.coinbase.com/en/base/social-feed/chat), [World](https://world.org/worldchat), and [Zora](https://support.zora.co/en/articles/8912129). The [XMTP ecosystem](https://xmtp.org/) includes 4,500+ developers and 1,700+ production miniapps.

XMTP supports tens of thousands of active conversations for a single user. Explore the [SDK performance suite](https://github.com/xmtp/libxmtp/blob/main/crates/xmtp_sdk/benchmarks/README.md) and [database conversation benchmark](https://github.com/xmtp/libxmtp/blob/main/crates/xmtp_db/benches/conversation_list.rs) for the measured workloads. See [benchmark commands](DEVELOPMENT.md#benchmarks) to run them.

## Start building an app

This README uses the version 8 release candidate, `8.0.0-rc2`, for the SDKs and messaging backend. Choose your platform below and follow the [installation guide](https://xmtp.org/get-started/install/) to get started. App developers can use the SDK packages directly.

Use an existing backend or [run the messaging backend](https://xmtp.org/get-started/run-the-backend/). For a Node.js app, install the SDK in your app project:

```sh
npm install --save-exact @xmtp/node-sdk@8.0.0-rc2
```

Follow the [chat app quickstart](https://xmtp.org/get-started/quickstart/) to create a signer and client with a 32-byte database encryption key. Create a conversation with a registered recipient's inbox ID. Send a message and receive it with the other client. Use the same backend for both clients.

For an agent, use the [Agent quickstart](https://xmtp.org/agents/quickstart/). It covers key setup, a text handler, and testing the agent. For a browser or mobile app, use the [chat app quickstart](https://xmtp.org/get-started/quickstart/) and select your platform.

See [Create a client](https://xmtp.org/sdk/client/) for backend and storage settings.

## SDKs and packages

Each version 8 RC SDK has source, an installation guide, and an API reference.

| SDK | Language | Source | Documentation | Package |
| --- | --- | --- | --- | --- |
| Browser | TypeScript | [Source](https://github.com/xmtp/libxmtp/tree/main/sdks/browser) | [Install](https://xmtp.org/get-started/install/#browser-node-and-agent-packages) · [API](https://xmtp.org/reference/browser-sdk/classes/client/) | [npm — @xmtp/browser-sdk](https://www.npmjs.com/package/@xmtp/browser-sdk) |
| Node | TypeScript | [Source](https://github.com/xmtp/libxmtp/tree/main/sdks/node) | [Install](https://xmtp.org/get-started/install/#browser-node-and-agent-packages) · [API](https://xmtp.org/reference/node-sdk/classes/client/) | [npm — @xmtp/node-sdk](https://www.npmjs.com/package/@xmtp/node-sdk) |
| Agent | TypeScript | [Source](https://github.com/xmtp/libxmtp/tree/main/sdks/agent) | [Guide](https://xmtp.org/agents/quickstart/) · [API](https://xmtp.org/reference/agent-sdk/classes/agent/) | [npm — @xmtp/agent-sdk](https://www.npmjs.com/package/@xmtp/agent-sdk) |
| Android | Kotlin | [Source](https://github.com/xmtp/libxmtp/tree/main/sdks/android) | [Install](https://xmtp.org/get-started/install/#android-package) · [API](https://xmtp.org/reference/kotlin/) | [Maven Central — org.xmtp:android](https://central.sonatype.com/artifact/org.xmtp/android) |
| iOS | Swift | [Source](https://github.com/xmtp/libxmtp/tree/main/sdks/ios) | [Install](https://xmtp.org/get-started/install/#swift-package) · [API](https://xmtp.org/reference/swift/documentation/xmtpsdk/) | [Swift Package Manager](https://github.com/xmtp/libxmtp) · [CocoaPods — XMTP](https://cocoapods.org/pods/XMTP) |

## Explore the examples

- [Web chat](https://github.com/xmtp/libxmtp/tree/main/apps/web-chat): Run a browser client against your backend.

- [Android Messenger](https://github.com/xmtp/libxmtp/tree/main/apps/example-android): Test messaging and debug the Android SDK.

- [iOS examples](https://github.com/xmtp/libxmtp/tree/main/apps/example-ios): Build a mobile client with the local Swift package.

- [React Native Messenger](https://github.com/xmtp/libxmtp/tree/main/apps/example-react-native): Build an iOS and Android client with the React Native SDK.

- [Agent demos](https://github.com/xmtp/libxmtp/tree/main/sdks/agent/src/demo): Explore message handlers and app-specific behavior.

For more Agent SDK examples, see [XMTP Agent Examples](https://github.com/xmtplabs/xmtp-agent-examples).

## Getting started working in this repo

The local setup uses Nix to provide pinned development tools. The setup script below targets macOS, with Apple Silicon as the primary platform. Install and start Docker for test services.

1. Clone the repository and create a branch for your change.

   ```sh
   git clone https://github.com/xmtp/libxmtp.git
   cd libxmtp
   git switch -c my-change
   ```

2. Set up Nix and the binary cache. Follow the script prompts.

   ```sh
   ./dev/nix-up
   ```

3. Start the backend and check the workspace.

   ```sh
   dev/nix-shell 'just backend up'
   dev/nix-shell 'just check'
   dev/nix-shell 'just test'
   ```

4. Run the shared checks. Then run the build and tests listed in the guide for the directory you changed.

   ```sh
   dev/nix-shell 'just lint'
   ```

`just test` covers the default Rust workspace members and excludes backend database tests. Run those tests with `dev/nix-shell 'just backend test'`. Run the dev tool tests with `dev/nix-shell 'just test dev'` after a change under `dev/`, `nix/`, or `.github/`. Use each SDK's guide for platform builds and tests. Run `dev/nix-shell 'just'` to list the available tasks.

To run the full backend and web example:

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just backend status'
dev/nix-shell 'just install-js'
dev/nix-shell 'just web-chat dev'
```

Open the URL printed by the app. See the [web chat guide](https://xmtp.org/tools/web-chat/) for connection settings. Use the addresses printed by `backend status`; worktrees have separate port blocks. See the [backend guide](https://xmtp.org/get-started/run-the-backend/) for configuration and service details.

## Find your way around

| Path | Contents |
| --- | --- |
| [sdks](https://github.com/xmtp/libxmtp/tree/main/sdks) | Public SDK source and platform packages |
| [crates](https://github.com/xmtp/libxmtp/tree/main/crates) | Shared Rust code for messaging, identity, storage, and cryptography |
| [apps/backend](https://github.com/xmtp/libxmtp/tree/main/apps/backend) | Ready to deploy messaging backend |
| [apps](https://github.com/xmtp/libxmtp/tree/main/apps) | Example apps, command-line tools, and developer tools |
| [docs/specs](https://xmtp.org/specs/) | Behavior specifications |
| [docs](https://xmtp.org/) | Setup, operations, and SDK guides |

## Contribute

See [DEVELOPMENT.md](DEVELOPMENT.md) for dependency changes, test logging, coverage, and benchmark commands.

Start with [CONTRIBUTING.md](https://github.com/xmtp/libxmtp/blob/main/CONTRIBUTING.md) and [AGENTS.md](https://github.com/xmtp/libxmtp/blob/main/AGENTS.md). Read the instructions in the directory you change. Check the [specifications](https://xmtp.org/specs/) for the relevant behavior.

Open an [issue](https://github.com/xmtp/libxmtp/issues) for a bug or to discuss a substantial change. Branch from `main` and target your pull request at `main`. Explain what changed and how you checked it. Include a test that detects the bug when you fix behavior, and show that the test fails without the fix.

For help building an app, see the [debugging guide](https://xmtp.org/sdk/debug/) or open an [issue](https://github.com/xmtp/libxmtp/issues). For the protocol's security properties, see the [security documentation](https://xmtp.org/protocol/security/).

## License

[MIT](https://github.com/xmtp/libxmtp/blob/main/LICENSE).
