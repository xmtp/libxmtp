# Onboarding

Welcome to XMTP protocol development! We recommend you skim this guide as a quickstarter, and then use it as a reference to come back to whenever you need to go deeper. Later in your journey, if there's ever a feature or concept you don't understand, please ping the team to add a link to the guide!

## High-level components

[<img src="https://github.com/user-attachments/assets/511c316f-ae19-4767-b4ef-946fb775e14b" />](https://link.excalidraw.com/l/4nwb0c8ork7/6XJqcEee1ah)

Here we describe each component, as well as providing links to the interface boundaries between components, which is the key to getting a high level understanding. Make sure to follow the README in each relevant repo if you want to set it up.

#### Backend Node

- Centralized repo: https://github.com/xmtp/xmtp-node-go
- Decentralized repo: https://github.com/xmtp/xmtpd

This is the backend, currently a set of centralized nodes operated by Ephemera, sharing a Postgres instance. The current API interface is captured in the [protos](https://github.com/xmtp/proto) repo, mostly [here](https://github.com/xmtp/proto/blob/8b46912744d89e29236f21ed81b33facb5384239/proto/identity/api/v1/identity.proto#L19) and [here](https://github.com/xmtp/proto/blob/8b46912744d89e29236f21ed81b33facb5384239/proto/mls/api/v1/mls.proto#L23). A simple mental model of the backend is a pub/sub system, with append-only, ordered lists of payloads categorized by [topic](https://docs.xmtp.org/protocol/topics) and indexed by [cursors](https://docs.xmtp.org/protocol/cursors).

A trustless, decentralized replacement is currently under development. More information can be found in the [explainer](https://xmtp.org/docs/concepts/decentralizing-xmtp) and [XIP](https://community.xmtp.org/t/xip-49-decentralized-backend-for-mls-messages/856).

#### Rust SDK (libxmtp)

- Libxmtp repo: https://github.com/xmtp/libxmtp
- OpenMLS repo (fork): https://github.com/xmtp/openmls

This is the core client SDK. It fetches and publishes payloads from the [backend](https://github.com/xmtp/proto/blob/8b46912744d89e29236f21ed81b33facb5384239/proto/identity/api/v1/identity.proto#L19), encrypting and decrypting them using [OpenMLS](https://book.openmls.tech/), and storing them in the [database](https://github.com/xmtp/libxmtp/blob/2ab5529d4bc0ca1aa90e986a78cb23d2c6f227b7/xmtp_db/src/encrypted_store/schema_gen.rs#L1) before exposing results to the native SDK's via [bindings](https://github.com/xmtp/libxmtp/tree/main/bindings_ffi).

It's recommended to start by understanding [envelope types](https://docs.xmtp.org/protocol/envelope-types) and [intents](https://docs.xmtp.org/protocol/intents), before moving onto deeper level concepts in the Core Concepts section below.

#### Validation Service

This is simply a library in the libxmtp repo for validating payloads that is used in both the backend and client.

#### Platform SDK's


- iOS repo: https://github.com/xmtp/xmtp-ios
- Android repo: https://github.com/xmtp/xmtp-android
- React Native repo: https://github.com/xmtp/xmtp-react-native
- JS (browser/node.js) repo: https://github.com/xmtp/xmtp-js
- Push notif server example: https://github.com/xmtp/example-notification-server-go

This is a set of SDK's for each native platform. The interface to Rust is described in the [bindings](https://github.com/xmtp/libxmtp/tree/main/bindings_ffi), and the user interface is described in the [docs](https://docs.xmtp.org). 

#### Reference apps and agents

- Convos (iOS) repo: https://github.com/ephemeraHQ/convos-ios
- xmtp.chat (web) repo: https://github.com/xmtp/xmtp-js/tree/main/apps/xmtp.chat
- Agents repo: https://github.com/ephemeraHQ/xmtp-agent-examples

These are open-source reference apps and bots developed by Ephemera. We recommend downloading Convos from the App store and loading xmtp.chat in your browser to get a feel for the types of features you can build on top of XMTP. Please ping a team member so you can DM with them in the app and get added to a developer's group. Outside of Ephemera, there are a wide variety of apps and agents from an array of organizations built on XMTP - please ask for a list, and experiment with them too.

## Core concepts

The XMTP protocol can be understood via three core concepts:

- MLS protocol
  - Go through 'libxmtp' section above
  - [Intuition](https://www.loom.com/share/a7450ecb62e84da78b39274eb4069351) and [formal talk](https://www.youtube.com/watch?v=FTPRjVLi8k4): Understand the general goals and mechanisms
  - [Overview](https://docs.xmtp.org/protocol/overview): Understand how the 'authentication service' and 'delivery service' plug into the protocol, browse through other pages as needed
  - [OpenMLS library](https://book.openmls.tech/): Familiarize yourself with common operations
  - [MLS spec](https://www.rfc-editor.org/rfc/rfc9420.html): Use as a reference for specifics later, read it end-to-end if dedicated (or feed it into an AI and chat about it)
- Identity protocol (authentication service)
  - [Explainer](https://xmtp.org/docs/concepts/identity) and [XIP](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-46-multi-wallet-identity.md)
- Message delivery protocol (delivery service)
  - See 'backend node' section above to understand the centralized system
  - Decentralization [explainer](https://xmtp.org/docs/concepts/decentralizing-xmtp) and [XIP](https://community.xmtp.org/t/xip-49-decentralized-backend-for-mls-messages/856) (still under development)

## Going deeper

Once the core concepts are well-understood, there is a vast number of additional topics to explore, non-exhaustive list below (please add links!):

- Protocol:
  - Content types
  - Device sync and consent
  - DM stitching
  - MLS group context and mutable metadata
  - Automated fork recovery
  - Disappearing messages
  - Push notifications
  - Post-quantum security
  - Bots and agents
  - SDK release process and versioning
  - Performance testing
- Decentralization:
  - Payer portal
  - Migration
