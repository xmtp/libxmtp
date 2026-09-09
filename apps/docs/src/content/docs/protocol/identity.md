---
title: Identity model
---

XMTP's identity model includes an inbox ID and its associated identities and installations.

| Term              | Meaning                                                       |
| ----------------- | ------------------------------------------------------------- |
| Inbox ID          | Stable destination for messages                               |
| Identity          | Addressable account linked to an inbox                        |
| Recovery identity | First identity; can manage other identities and installations |
| Installation      | One app instance with its own cryptographic keys              |

An inbox can contain several account identifiers. Messages to any linked identifier reach the same inbox. Supported identifiers include Ethereum EOAs, smart contract wallets, and passkeys.

Each installation has independent keys and local state. An inbox supports up to 10 installations. The recovery identity can revoke installations.

```text
Inbox ID (stable destination for messages)
├── Identity 1 (recovery identity, first identity added to an inbox)
├── Identity 2 (EOA wallet)
├── Identity 3 (SCW wallet)
└── Any identity that can produce a verifiable cryptographic signature
```

```text
Each identity can authenticate new installations:
├── Installation A (phone app)
├── Installation B (web app)
├── Installation C (desktop app)
└── Up to 10 installations
```

See [inboxes and installations](/sdk/inboxes/) for the SDK operations and limits.
