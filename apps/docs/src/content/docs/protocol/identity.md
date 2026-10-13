---
title: Identity model
---

XMTP's identity model includes an inbox ID, its associated identities and installations, and its handles.

| Term              | Meaning                                                       |
| ----------------- | ------------------------------------------------------------- |
| Inbox ID          | Stable destination for messages                               |
| Identity          | Addressable account linked to an inbox                        |
| Recovery identity | First identity; can manage other identities and installations |
| Installation      | One app instance with its own cryptographic keys              |
| Handle            | Public name that an inbox claimed, such as `nick#2341`        |

An inbox can contain several account identifiers. Messages to any linked identifier reach the same inbox. Supported identifiers include Ethereum EOAs, smart contract wallets, and passkeys.

Each installation has independent keys and local state. The default limit is 10 installations per inbox. The backend can publish a different limit through `mls.max_installations_per_inbox`. The recovery identity can revoke installations.

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
└── Up to the deployment's installation limit
```

See [inboxes and installations](/sdk/inboxes/) for the SDK operations and limits.

## Handles

An inbox claims a handle with a `ClaimHandle` action in its identity log. Any current member signs the claim: an installation, an Ethereum account, or a passkey. A handle is not an identity: it signs nothing. Its name part has 1 to 27 characters: a lowercase letter, then lowercase letters, digits, `_`, or `-`. An optional `#` and 4 digits follow. Validators reject any other spelling and never rewrite a signed handle. One update carries at most one claim, and one log holds at most 64 claims. The last claim is the display handle. Every client checks a handle lookup answer against the inbox's own log, so a backend cannot point a handle at an inbox that did not claim it.

Handles are public and permanent. Every client reads the whole identity log, and anyone who can query the deployment can read it, so every claim stays readable. No action removes a handle, so an old handle still finds its inbox. Only the backend makes a handle unique. It applies its reserved names, its minimum name length, and 7 days between claims only when it admits a claim.

See [Handles](/sdk/inboxes/#handles) for the SDK calls that claim, find, and show handles.
