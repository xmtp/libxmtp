---
title: Epochs with XMTP
---

With XMTP, each [commit](/protocol/envelope-types/#group-messages) starts a new epoch, which represents the current cryptographic state of a group chat.

Epochs are a core concept from [Messaging Layer Security](https://messaginglayersecurity.rocks/) (MLS), which XMTP implements for secure group messaging.

Epochs work according to these requirements:

- Sequential numbering: Epochs are strictly ordered and increase by one with each commit (epoch 1, epoch 2, etc.).
- New keys: Each epoch introduces a fresh encryption key, and the SDK retains secrets for three preceding epochs to process delayed application messages. This is a fixed SDK limit, not an operator setting. Older epoch secrets are discarded.
- Decryption requirement: To read messages or commits in a given epoch, a member must have the correct epoch key.
- Fork risk: Members that apply different commits at the same epoch can fork. A member that only falls behind can catch up by processing the ordered commit history.

[Intents](/protocol/intents/) help ensure two types of ordering:

- **Epoch ordering**: A commit must be built for the current epoch. Applying it advances the group by one epoch.
- **Consistent ordering**: All clients must receive published commits in the same order to prevent forks, regardless of epoch validity

Intents achieve epoch ordering by enabling retries, while relying on the backend's guarantee of consistent ordering for each topic. Order across topics is undefined.

## Handle concurrent commits

When multiple commits arrive at nearly the same time, Clients process commits in backend topic order. For example, if commits 2 and 3 both attempt to advance from epoch 1, the first valid commit in topic order advances the group to epoch 2. The other commit is then stale and is rejected by clients. The backend does not choose or validate the winning MLS commit.

For example, if commit 2 arrived first, clients apply it to the epoch 1 state and advance to epoch 2. Commit 3, which was also built for epoch 1, is rejected.

However, [intents](/protocol/intents/) provide a mechanism for the rejected commit 3 to be retried. The intent can be reprocessed against the new epoch 2 state, so the SDK can retry the operation. A permanent error can still stop the intent.
