---
title: MLS protocol overview
---

XMTP enables secure, end-to-end encrypted communication between identities that can produce a verifiable cryptographic signature.

XMTP implements [Messaging Layer Security](https://messaginglayersecurity.rocks/) (MLS), which is designed to operate within the context of a messaging service. As the messaging service, XMTP needs to provide two services to facilitate messaging using MLS:

- An authentication service
- A delivery service

The identity system provides authentication. The self-hosted backend provides ordered storage and delivery. The SDK handles encryption, identity checks, topic construction, cursors, and retries.

Use this section to understand [security](/protocol/security/), [envelope types](/protocol/envelope-types/), [topics](/protocol/topics/), [epochs](/protocol/epochs/), [intents](/protocol/intents/), [cursors](/protocol/cursors/), and [identity](/protocol/identity/).
