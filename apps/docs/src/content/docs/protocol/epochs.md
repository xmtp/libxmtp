---
title: Epochs with XMTP
---

With XMTP, each [commit](/protocol/envelope-types/#group-messages) starts a new epoch, which represents the current cryptographic state of a group chat.

Epochs are a core concept from [Messaging Layer Security](https://messaginglayersecurity.rocks/) (MLS), which XMTP implements for secure group messaging.

Epochs work according to these requirements:

- Sequential numbering: Epochs are strictly ordered and increase by one with each commit (epoch 1, epoch 2, etc.).
- New keys: Each epoch introduces a fresh encryption key, and keys from previous epochs are discarded (with the exception of a configurable number of recent past epochs that may be retained for special cases).
- Decryption requirement: To read messages or commits in a given epoch, a member must have the correct epoch key.
- Fork risk: If members diverge on which epoch they're in (e.g., due to missed or out-of-order commits), they won't be able to decrypt each other's messages, causing a fork.

[Intents](/protocol/intents/) help ensure two types of ordering:

- **Epoch ordering**: Commits must be published in the correct epoch (one greater than the last applied epoch) to be processed by clients
- **Consistent ordering**: All clients must receive published commits in the same order to prevent forks, regardless of epoch validity

Intents achieve epoch ordering by enabling retries, while relying on the backend's guarantee of consistent ordering for each topic. Order across topics is undefined.

## Handle concurrent commits

When multiple commits arrive at nearly the same time, XMTP uses a "first-to-arrive wins" approach. For example, if commits 2 and 3 both attempt to advance from epoch 1, whichever commit arrived first becomes the accepted next epoch, and the other commit is rejected.

For example, if commit 2 arrived first, it will be built on epoch 2 and commit 3 will be rejected.

However, [intents](/protocol/intents/) provide a mechanism for the rejected commit 3 to be retried. The intent can be reprocessed against the new epoch 2 state, allowing the operation to succeed in the updated context rather than being permanently lost.
