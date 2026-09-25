# Glossary

The actors and shared terms every spec uses. A spec's own Terms section defines only terms local to that spec. Add an actor here, not in a spec (SPEC-038).

This file names things; it does not constrain them. Under SPEC-072 every obligation belongs to a requirement in the spec that owns the capability, so an entry here says what a term refers to and points at the spec that states its rules. A property worth enforcing, such as how sequence ids are ordered or how a message hash is computed, is a requirement there and not a sentence here.

## Actors

| Actor | Meaning |
| --- | --- |
| The backend | The self-hosted XMTP server: the gRPC services in the `xmtp.backend.v1` package and the process that serves them. |
| The client | libxmtp: the Rust library that holds keys, MLS state, and the local database, on native and wasm targets. |
| An SDK | A language binding on top of the client: Kotlin, Swift, or TypeScript. Specs describe SDK behaviour abstractly, not by method name. |
| An app | The developer's program that uses an SDK. |
| An operator | The party that deploys and configures a backend. |
| A member | An inbox that belongs to a group. "Another member" is a member other than the one whose client is acting. |
| A peer installation | Another installation of the same inbox as the acting client. |
| A sender | The installation that published an envelope. |
| A recipient | An installation the backend delivers an envelope or a push notification to. |
| A validator | Whichever party checks a payload, when the obligation is the same for the backend and the client. Use a named actor when only one of them is bound. |

## Terms

| Term | Meaning |
| --- | --- |
| Inbox | The identity a user acts as, named by an inbox id. Derivation and length are stated by `IDENT`. |
| Installation | One client instance with its own key pair, named by an installation key. An inbox has many. Stated by `IDENT`. |
| Identifier | An external account bound to an inbox: an Ethereum address, a smart contract wallet, or a passkey. |
| Envelope | One unit of client data the backend stores: a group message, a Welcome, a key package, an identity update, or a commit-log entry. |
| Topic | The routing key of an envelope. Its layout is stated by `TOPIC`. |
| Sequence id | The integer the backend assigns to a stored envelope. Its range, ordering, and uniqueness are stated by `API`. |
| Cursor | A client's position on one topic. Its meaning and bounds are stated by `API`. |
| Message hash | The digest that identifies a stored envelope. Its algorithm and role in idempotency are stated by `API`. |
| Group | An MLS group: a conversation named by a group id. A DM, a sync group, and a one-shot group are groups with a conversation type. |
| DM | A group between exactly two inboxes with the fixed DM policy. Owned by the `DMS` spec. |
| Epoch | The MLS epoch of a group. Every commit advances it. |
| Commit | An MLS message that changes a group's state and advances its epoch. |
| Proposal | An MLS message that proposes a change for a later commit. |
| Welcome | The MLS message that adds an installation to a group, inline or through a welcome pointer. |
| Key package | An installation's published MLS credential and keys that let another member add it to a group. |
| Identity update | A signed change to an inbox's association log. Owned by the `IDENT` spec. |
| Commit-log entry | A signed record of a commit that a client publishes for fork detection. Owned by the `FORK` spec. |
| Consent | An inbox's or conversation's state of allowed, denied, or unknown. Owned by the `CONS` spec. |
| Content type | The typed encoding of a message payload. Owned by the `CTYPE` spec. |
| Remote attachment | The description of a file a message carries by reference: the URL of an encrypted object, with the key material and digest to fetch and decrypt it. Its encoding is owned by `CTYPE`; its storage, upload, and download by `ATCH`. |
