---
prefix: SEND
status: draft
---
# Message sending

The path from an app call to a published envelope, and from the published envelope back to a confirmed message. An app needs to know what it can observe after a send fails, and whether retrying can duplicate a message.

A send is durable before it is attempted. The client stores the message and an intent for it in one transaction, then prepares the MLS message once, stores the exact bytes it will send, and sends them. A retry resends those bytes, so the backend stores one envelope however many times the client asks. The send is confirmed by the same ordered processing that handles every other envelope (PROC): the client's own envelope comes back on the group topic, is matched to its attempt, and only then is the message published to the app and the send reported successful.

```mermaid
flowchart LR
  A[App call] -->|one transaction| S[(Message: Unpublished<br/>Intent: to publish)]
  S -->|prepare once| P[(Attempt: exact bytes<br/>Intent: published)]
  P -->|send| B[The backend]
  B -->|receipt| P
  B -->|echo on the group topic| O[Ordered processing<br/>PROC]
  O -->|match by payload hash| C[(Message: Published<br/>Intent: committed)]
  C -->|owed Welcomes sent| D[Intent: processed<br/>send returns]
```

## Scope

In scope: what an app's send stores before any request; the message id and the idempotency key; preparing an attempt once and retrying its exact bytes; publish receipts; the order in which a group's intents publish; confirmation through ordered processing; what an app observes for each failure; and what becomes of an intent that can never publish.

Out of scope: publish atomicity and duplicate detection on the backend (`?API`); receipt and ordered processing of the echo (PROC); what a commit contains and how it is validated (`?GMOD`); building and wrapping Welcomes (JOIN); the guarded app-data write (`?META`); the content encoding inside a message (`?CTYPE`); consent recorded on send (`?CONS`); and push flags (`?PUSH`).

| Related | Relation |
| --- | --- |
| PROC | Owns the receipt path, ordered processing, and delivery to app streams. This spec owns what a send stores, what it sends, and how the echo resolves it. |
| `?API` | Owns the publish RPC, the `message_hash` the backend assigns, and duplicate detection by that hash. This spec owns what the client sends and resends. |
| `?GMOD` | Owns the content and validation of commits. This spec owns how a commit intent is queued, published, and resolved. |
| `?META` | Owns the guard on an app-data write. This spec owns what a guard miss does to the intent. |

## Terms

| Term | Meaning |
| --- | --- |
| Intent | A durable record of one change an app asked for in one group: a message to send or a state change to commit. |
| Message intent | An intent whose payload is an application message. |
| State-change intent | An intent whose payload is a commit or a proposal: a membership, metadata, permission, admin, key, or app-data change. |
| Attempt | The exact envelope bytes prepared for an intent, together with the epoch and pending proposals they were built from. An intent has at most one current attempt. |
| Receipt | The backend's publish response for an attempt: the sequence id, timestamp, and `message_hash` of the stored envelope. |
| Echo | The client's own published envelope as ordered processing receives it on the group topic. |
| Payload hash | SHA-256 over the MLS message bytes inside an envelope. The key by which an echo is matched to its attempt. |
| Idempotency key | The `idempotency_key` of the `PlaintextEnvelope` a message is sent in: a string the app supplies, or 16 random bytes in hexadecimal when it does not. |
| Message id | The 32-byte identifier of a stored message, derived under SEND-002. |
| Rejected intent | An intent ordered processing or preparation refused for a reason a later attempt cannot change. |
| Superseded intent | A guarded app-data intent whose guard no longer matched when it was prepared. |

## 1. The send is stored first

An app's send writes before it sends. The client stores the message with `Unpublished` status and a message intent for it in one transaction, and returns the message id from that transaction. Nothing about the send is in memory only: a crash after the return leaves an intent the next sync publishes, and the app can read the message back by its id at once. PROC-025 keeps the unpublished message off the app's message stream until its echo confirms it.

A message is sent inside a `PlaintextEnvelope`, whose `idempotency_key` distinguishes two sends of the same bytes. The message id is derived from the group id, that key, and the content bytes, and nothing else: not the epoch, not the ciphertext, and not the time. Every installation in the group derives the same id from the same envelope, so a reply or a reaction that names a message id names the same message everywhere, and a message the client stores twice, once optimistically and once from its echo, is one row.

```proto
message PlaintextEnvelope {
  // Version 1 of the encrypted envelope
  message V1 {
    // Expected to be EncodedContent
    bytes content = 1;
    // A unique value that can be used to ensure that the same content can
    // produce different hashes. May be the sender timestamp.
    string idempotency_key = 2;
  }
  // V2 omitted: it carries the same idempotency_key with a oneof content.

  oneof content {
    V1 v1 = 1;
    V2 v2 = 2;
  }
}
```

The key is random by default, so two calls with the same content are two messages. An app that retries a send after an error it cannot interpret supplies its own key, or it sends the message twice.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| SEND-001 | Store before sending | When an app sends a message, the client MUST store the message with `Unpublished` status and a message intent for it in one transaction, and MUST return the message id to the app before it sends any request. | A send held only in memory is lost with the process, and the app has no id to find it by. |
| SEND-002 | Message id derivation | When a client stores an application message, whether it sent or received it, it MUST set the message id to SHA-256 over the group id, the byte `0x09`, the `idempotency_key` bytes, the byte `0x09`, and the `content` bytes of the `PlaintextEnvelope`, in that order. | Two installations that derive different ids for one message cannot agree on what a reply refers to. |
| SEND-003 | Same key, same message | When an app sends content and an idempotency key that equal a stored message's, the client MUST return that message's id, MUST NOT store a second message, and MUST NOT queue a second intent while an intent for that message is not yet applied or rejected. | A second intent for one id publishes the message twice. |
| SEND-004 | Retries supply the key | When an app retries a send after a result other than success, the app SHOULD supply the idempotency key of the first send. | |
| SEND-005 | No send into an inactive group | When an app sends into a group whose local state is inactive because a commit removed this installation, the client MUST refuse before it stores a message or an intent, with an error the app can distinguish. | A message stored for a group the client has left is never published and never fails. |

## 2. Preparing and publishing an attempt

Publishing an intent has two halves, and the boundary between them is what makes a retry safe. The first half runs under the state writer: the client builds the MLS message from the current group state, which advances its sender ratchet, and in the same transaction stores the exact envelope bytes, their payload hash, and the epoch and pending proposals they were built from. Only after that commits does the second half send the bytes. A crash between the two leaves an attempt the next round resends, and never a ratchet advanced for bytes nobody holds.

While an attempt has no receipt, its outcome is unknown: the request may have failed before the backend saw it, or the response may have been lost after the backend stored it. The client resends the same bytes. `?API` is expected to require that the backend identifies a stored envelope by the SHA-256 of its encoded bytes as `message_hash`, and that a publish of bytes it already holds returns the stored envelope's metadata rather than storing it again. Under that rule, a resend costs nothing and a re-encryption costs a second envelope with a second ratchet generation, which every recipient would decrypt as a second message.

An attempt is replaced only when ordered processing has resolved it: its echo was applied, its echo named a stale epoch (section 4), or the intent was rejected. A request that fails before it is sent, because the envelope exceeds the published size or the request is malformed, cannot succeed on a resend, so the intent is rejected then and later intents proceed. CONF-073 owns the size bound the client applies.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| SEND-006 | Prepare once, durably | When the client prepares an intent, it MUST build the MLS message once and store the exact envelope bytes, their payload hash, and the epoch they were built at, in one transaction that commits before any request is sent. | A crash between encryption and storage advances the sender ratchet for a message nobody holds, and every later message is out of step. |
| SEND-007 | Resend the same bytes | While an attempt has no receipt, the client MUST resend that attempt's stored bytes on every retry, and MUST NOT build a new MLS message for the intent until ordered processing has applied or rejected the attempt's echo. | A second encryption is a second envelope, which every recipient stores as a second message. |
| SEND-008 | A receipt binds to its attempt | When a publish response arrives, the client MUST attach it only to the attempt whose bytes were sent, MUST store the response's `message_hash` unchanged as the message's envelope hash, and MUST discard a response for an attempt that is no longer current. | A late response attached to a newer attempt confirms bytes the backend never stored. |
| SEND-009 | An unsendable request is rejected at once | If an attempt cannot be sent because an envelope exceeds `max_envelope_bytes` or the request is malformed, then the client MUST mark the intent rejected, MUST set its message to `Failed` status, and MUST continue with the group's later intents. | A message that can never be sent would otherwise block every message behind it, and show as pending for ever. |

## 3. Order within a group

One installation publishes a group's intents in the order the app queued them. Message intents publish in that order, several in one request when they are adjacent. A state-change intent is different: it produces a commit against a specific epoch, and until its echo says whether that commit landed, the client does not know what epoch the next intent must be built against. A published state-change intent therefore holds every intent behind it, and the client prepares at most one state-change attempt per group at a time.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| SEND-010 | Publish in queue order | The client MUST publish a group's intents in the order they were queued, and MUST NOT prepare any intent for a group while a state-change intent of that group is published and its echo is not yet applied or rejected. | A message built before the pending commit resolves is built against an epoch that may not exist. |

## 4. Confirmation

A publish receipt says the backend stored the envelope. It does not say the group accepted it: a commit can be superseded by another member's commit that landed first, and a message is not in the group's history until the client has applied its own envelope in sequence. The send therefore waits for its echo through ordered processing, with the receipt's sequence id as the target under PROC-015. Success is the intent applied, and for a membership commit, the Welcomes it owes sent.

The echo is matched to its attempt by payload hash, computed over the MLS message bytes the envelope carries. The receipt is not needed for the match, so an echo that arrives before the response still resolves the attempt. When the echo of a commit arrives at an epoch that is no longer the group's, another commit won the epoch: the attempt is discarded with its staged commit, and the intent is prepared again from the state that landed. `?GMOD` owns what a commit built from the new state contains.

An attempt with a receipt whose echo has not been applied within the deadline is neither a success nor a failure. The client reports it as published but unconfirmed, keeps the attempt current, and resolves it on the next sync. A client that retried it as a new send would produce a duplicate; one that reported failure would have the app resend a message the group already has.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| SEND-011 | Success is the applied echo | The client MUST report a send successful only when ordered processing has applied the intent's echo and, for a state-change intent that owes Welcomes, the backend has acknowledged every Welcome the client prepared for it, and MUST NOT report success on a receipt alone. | A commit the backend stored can still lose the epoch, and a success reported on the receipt tells the app the change landed when it did not. |
| SEND-012 | Match the echo by payload hash | When ordered processing receives an envelope whose sender is this installation, the client MUST match it to an attempt by the SHA-256 of the envelope's MLS message bytes equal to the attempt's payload hash, and MUST NOT require the receipt to make the match. | An own envelope processed as a stranger's cannot be decrypted, so the message is rejected and never confirmed. |
| SEND-013 | Confirmation completes the message | When the client applies a message intent's echo, it MUST set the message's status to `Published`, its sequence id to the envelope's, and its sent timestamp to the backend's timestamp on the envelope, in the transaction that applies the envelope. | A timestamp taken from the sender's clock differs on every installation, so members disagree on the order of messages. |
| SEND-014 | A stale commit is prepared again | When a state-change intent's echo names an epoch other than the group's current epoch, the client MUST discard the attempt and its staged commit, MUST NOT apply that commit, and MUST prepare the intent again from the current state. | Applying a commit built against a superseded epoch forks the group. |
| SEND-015 | Published but unconfirmed | When the client holds a receipt for an attempt and ordered processing has not applied its echo within 60 seconds of the send call, the client MUST return a result the app can distinguish from success and from failure, MUST keep the attempt current, and MUST NOT prepare a replacement attempt. | Reporting failure has the app send the message again; replacing the attempt sends it again itself. |
| SEND-016 | A rejected echo rejects the intent | When ordered processing records a terminal rejection for an intent's echo for a reason other than a stale epoch, the client MUST mark the intent rejected with a stable reason code and MUST return that reason to the send. | An app that cannot tell a rejected send from a slow one retries what the group refused. |

## 5. Intents that cannot publish

Three things end an intent without a successful echo. A rejection under SEND-009 or SEND-016. A guard miss: `?META` is expected to own a guarded app-data write whose guard names the value it expects, and an intent whose guard no longer matches when it is prepared is superseded, which is not an error but is terminal. And removal: a commit that removes this installation makes every unpublished or unresolved intent of that group unpublishable, and the messages behind them are failed so that the app does not show them as pending for ever. An intent whose commit was applied before the removal still owes its Welcomes and completes them.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| SEND-017 | A superseded guard is terminal | When the client prepares a guarded intent whose guard no longer matches the committed value, it MUST mark the intent superseded without publishing, MUST NOT retry it, and MUST return a result the app can distinguish from a rejection. | Retrying a stale write spins until the deadline; reporting it as an error tells the app something broke when nothing did. |
| SEND-018 | Removal fails pending intents | When the client applies a commit that removes this installation from a group, it MUST mark every intent of that group that is unpublished, or published with no applied echo, as rejected, and MUST set each such message intent's message to `Failed` status, in the transaction that applies the removal. | A message left `Unpublished` in a group the client has left is shown as pending for ever. |
| SEND-019 | Failures are distinguishable | An SDK MUST let an app distinguish, as distinct error kinds, an inactive group (SEND-005), an unsendable request (SEND-009), a rejected intent with its reason (SEND-016), a superseded intent (SEND-017), a send that is published but unconfirmed (SEND-015), and a send that failed before any receipt and may be retried. | |

## Known limitations

An intent has no attempt limit and no deadline. An attempt whose echo never arrives is retried on every sync with the same bytes, and a published state-change intent whose echo never resolves holds every later intent of its group (SEND-010) until it does. Nothing ages such an intent out; only a removal (SEND-018) ends it.

The idempotency key is random by default. An app that retries a send without supplying the key of the first send publishes the same content twice under two message ids (SEND-004).

A message's sent timestamp changes when it is confirmed (SEND-013). An app that orders messages by that timestamp before confirmation sees the message move.

Terminal intents are never deleted. A rejected, superseded, or processed intent stays in the client database with its stored attempt.

A rejection reason is a stable code without its parameters. An app learns that a commit was refused for permissions, not which member or field.
