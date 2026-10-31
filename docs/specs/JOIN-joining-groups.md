---
prefix: JOIN
status: draft
---
# Joining groups

Everything between publishing a key package and holding group state the client can send and read in. The rules on when a Welcome may replace existing state decide whether a member can still decrypt after a re-add.

A join is the one operation in which an installation accepts a group's membership and state from a single message it did not help produce. Every other group operation is checked against state the client already holds. The rules here supply the checks a Welcome gets instead: the sender is read from the ratchet tree, the membership is checked against identity state the client resolves itself, and the join is tied to a position in the group's message topic that precedes the Welcome.

```mermaid
flowchart TD
  P[Publish key package] --> W[Welcome arrives]
  W --> U{Inline or pointer}
  U -->|inline| D[Unwrap with the installation's private key]
  U -->|pointer| R[Resolve the pointee, then unwrap symmetrically]
  R --> D
  D --> A[Check the join anchor]
  A --> M[Validate initial membership]
  M --> S{Local state for this group}
  S -->|none| J[Install]
  S -->|exists| E[Apply the replacement rules]
  E --> J
  J --> V[Validate the group at join time]
```

## Scope

In scope: what a published key package carries and advertises, its lifetime and rotation, when superseded key material is destroyed, unwrapping a Welcome under either wrapper, welcome pointers and their resolution, the join anchor, when a Welcome may replace existing local group state, validation of the membership a Welcome asserts, and the group properties a joiner checks before it accepts.

Out of scope: steady-state commit validation and how a membership change is proposed and committed (`GMOD`), the association log a joiner resolves membership against (`IDENT`), the DM identifier and stitching (`DMS`), consent, which no join stores (`CONS`), the sync conversation's own trust rule (`SYNC`), the policy engine (`PERM`), the metadata model (`META`), topic layout (`TOPIC`), and the publish and query contract (`API`).

| Related | Relation |
| --- | --- |
| `EVENT` | EVENT-001 reports a joined conversation after the client stores it. EVENT-005 and EVENT-023 own the join origin and adder in the event payload. |
| `DMS-001` | Owns the dm id that JOIN-089 compares with a held group's stored dm id. |

## Terms

| Term | Meaning |
| --- | --- |
| Wrapper | The encryption applied to a Welcome or a welcome pointer so that only the addressed installation can read it, in addition to MLS's own encryption of the Welcome. |
| Wrapper algorithm | The `wrapper_algorithm` value a sender sets on a published Welcome or welcome pointer, which names the private key the recipient decrypts with. |
| Welcome pointer | A Welcome variant that carries the topic of a pointee and a symmetric key instead of the Welcome itself. |
| Pointee | The Welcome that a welcome pointer names, shared by every installation the same commit adds. |
| Join anchor | The `message_cursor` a Welcome carries: the sequence id, on the group's message topic, of the commit that added the recipient. |
| Superseded key package | A published key package that a later published key package from the same installation replaces. |
| Terminal rejection | The refusal a client records for an envelope under JOIN-048. |

## 1. What a key package carries

A key package is a public resource that every installation on the network produces. It carries the configuration and key material another installation needs to perform a handshake with this installation: the inbox it belongs to, its installation signature key, its MLS `init_key`, the extension and proposal types it supports, and the wrapper a sender is to encrypt a Welcome under.

A key package is an RFC 9420 [`KeyPackage` (§10)](https://www.rfc-editor.org/rfc/rfc9420.html#section-10) in TLS presentation language. It travels inside `KeyPackage.key_package_tls_serialized` below. A Welcome is an RFC 9420 [`MLSMessage` (§6)](https://www.rfc-editor.org/rfc/rfc9420.html#section-6) whose body is a [`Welcome` (§12.4.3.1)](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.4.3.1); it travels inside `WelcomeMessage.V1.data` (section 4) under the wrapper. Neither format is redefined here.

```proto
// Last-resort key package for one installation. The server validates it and
// derives the topic from its installation key. Every upload is kept; the
// newest per installation is served by QueryNewest. Query with a cursor on a
// key-package topic is not supported.
message KeyPackage {
  bytes key_package_tls_serialized = 1;
}
```

The inbox is carried in the leaf node's credential. The credential is an RFC 9420 [`basic` credential (§5.3)](https://www.rfc-editor.org/rfc/rfc9420.html#section-5.3) whose `identity` is a serialized `MlsCredential`:

```proto
// A credential that can be used in MLS leaf nodes
message MlsCredential {
  string inbox_id = 1;
}
```

A key package carries two XMTP extensions inside the MLS object, under extension types RFC 9420 does not define. The identifiers below are the interoperability contract: an advertisement written under any other identifier is invisible to every other implementation.

```proto
// The KeyPackageExtension that stores the PubKey and the WelcomeWrapperEncryption
message WelcomeWrapperEncryption {
  bytes pub_key = 1;
  WelcomeWrapperAlgorithm algorithm = 2;
}
```

```proto
// Extension message that indicates the types of encryption supported by a client
message WelcomePointeeEncryptionAeadTypesExtension {
  repeated WelcomePointeeEncryptionAeadType supported_aead_types = 1;
}
```

| Extension | Identifier | Carries |
| --- | --- | --- |
| Wrapper encryption | `0xff03` | A `WelcomeWrapperEncryption`: the public key and the wrapper algorithm a sender encrypts a Welcome for this installation under. |
| Welcome pointee encryption | `0xff04` | A `WelcomePointeeEncryptionAeadTypesExtension`: the symmetric algorithms this installation accepts for a pointee. |

A validator reads `cipher_suite` from the key package and verifies its signatures under the signature scheme of that suite, as [RFC 9420 §10.1](https://www.rfc-editor.org/rfc/rfc9420.html#section-10.1) requires. XMTP fixes the suite an installation publishes under, so a package under any other suite cannot be added to a group. The lifetime check in [§7.3](https://www.rfc-editor.org/rfc/rfc9420.html#section-7.3) is RECOMMENDED for a received leaf node; XMTP requires it for a fetched key package (JOIN-008) and omits it for the leaf nodes inside a Welcome's ratchet tree (Known limitations). XMTP also applies one rule of the MLS extensions draft: the `last_resort_key_package` component (`0x0004`) of a key package's `app_data_dictionary` extension only marks the package, so its `data` is empty ([draft-ietf-mls-extensions-10 §6.4.2](https://datatracker.ietf.org/doc/html/draft-ietf-mls-extensions-10#section-6.4.2)), and a validator rejects a package whose component carries data (JOIN-082).

A sender fetches the newest key package the backend serves for each installation; the backend serves only that one. Using an older package would not break a protocol guarantee, but its keys may already be destroyed under section 3. When an inbox has some installations with a valid package and some without, the sender adds the inbox with the valid ones (JOIN-093) and records the rest (JOIN-012), so that a joiner can distinguish an installation the sender could not add from one it left out.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| JOIN-001 | Key package names its inbox | An installation MUST publish only a key package whose credential names exactly one inbox and whose leaf node `signature_key` is an installation key that the named inbox's identity state associates with that inbox. | |
| JOIN-002 | Advertised capability is binding | An installation MUST be able to process every extension type and proposal type its published key package lists in its leaf node `Capabilities` ([RFC 9420 §7.2](https://www.rfc-editor.org/rfc/rfc9420.html#section-7.2)). | A sender configures the group from what the recipients advertise. An installation that lacks a capability it advertised forks the group at the first message that uses it. |
| JOIN-004 | One credential encoding | An installation MUST carry its inbox in its key package as a `basic` credential whose `identity` is a serialized `MlsCredential` as defined above, and MUST NOT use any other credential type or encoding. | |
| JOIN-007 | Reject an unverifiable package | When a key package fails the validation in [RFC 9420 §10.1](https://www.rfc-editor.org/rfc/rfc9420.html#section-10.1), including the leaf node validation of [§7.3](https://www.rfc-editor.org/rfc/rfc9420.html#section-7.3) it invokes, a validator MUST reject it and MUST NOT use it to address an installation. | |
| JOIN-008 | Reject an expired package | When the validator's current time is outside the `Lifetime` of the key package's leaf node ([RFC 9420 §7.2](https://www.rfc-editor.org/rfc/rfc9420.html#section-7.2)), at or after `not_after` or before `not_before`, the validator MUST reject the key package. | |
| JOIN-082 | Reject a last-resort mark with data | When a key package's `app_data_dictionary` extension holds the `last_resort_key_package` component (`0x0004`) with non-empty `data`, a validator MUST reject the key package and MUST NOT use it to address an installation. | Validators that follow the extensions draft reject such a package, also inside an Add. A commit that adds one is applied by the members that accept the package and rejected by those that do not, and the group forks. |
| JOIN-090 | Reject an unusable addressing extension | When a key package's `0xff03` or `0xff04` extension does not decode, or its wrapper (the `0xff03` extension, or `WELCOME_WRAPPER_ALGORITHM_CURVE25519` under `init_key` when that extension is absent) is neither `WELCOME_WRAPPER_ALGORITHM_CURVE25519` with a 32-byte key nor `WELCOME_WRAPPER_ALGORITHM_XWING_MLKEM_768_DRAFT_6` with a 1,216-byte key, or an HPKE sender setup under that algorithm fails for that key, the backend at admission and a sender choosing the installations to add MUST reject the key package. Both MUST treat a `supported_aead_types` value other than `WELCOME_POINTEE_ENCRYPTION_AEAD_TYPE_CHACHA20_POLY1305` as absent and MUST NOT reject a package for it. A member processing a commit MUST NOT reject the commit because a key package it adds fails this row. | A package no sender can address is otherwise stored and served as the installation's newest package (API-244), and every sender that fetches it fails the add, or fails the Welcomes of a commit it has already applied. Rejecting an AEAD value defined later makes every installation that advertises it unaddable by older clients, and members that disagree about a key package in a commit fork the group. |
| JOIN-072 | Published ciphersuite | An installation MUST publish its key package with `cipher_suite` `MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519`. | A package under another suite passes signature verification and then fails at every add, because the group's suite does not match. |
| JOIN-093 | One bad installation | When an inbox has at least one installation whose key package passes JOIN-007, JOIN-008, JOIN-082 and JOIN-090, has the `cipher_suite` JOIN-072 names, has that installation's key as its leaf node `signature_key`, and has a credential that names that inbox, a sender MUST add that inbox using those installations and MUST NOT fail the operation because another installation of the same inbox has no such package. | One broken or stale device would otherwise make its owner unaddable by anyone. A package whose credential names another inbox is unusable, because every member rejects its leaf under GMOD-012. A package under another suite fails every Add, and API-235 makes the backend serve it. A package whose leaf is another installation's key adds the wrong installation, and joiners then reject the Welcome under JOIN-053. |
| JOIN-012 | Record what was left out | When a sender adds an inbox without one of its installations, the sender MUST record that installation's installation key in `failed_installations` of that inbox's entry in the group's `GROUP_MEMBERSHIP` component. | Without the record, JOIN-053 rejects the Welcome on every joiner, or a silently dropped device is indistinguishable from one the sender never saw. |

## 2. Lifetime and rotation

MLS as deployed elsewhere uses each key package once: an installation uploads many, the delivery service hands each one out once and then deletes it, and a reusable "last resort" package is the fallback ([RFC 9420 §16.8](https://www.rfc-editor.org/rfc/rfc9420.html#section-16.8)). XMTP is permissionless. Anyone can fetch an installation's key packages, so an attacker can exhaust a supply in one pass and leave the installation with nothing but its last-resort package. XMTP therefore treats the last-resort workflow as the standard one: an installation publishes one key package at a time, every sender uses that package, and the installation bounds the package's exposure with a lifetime and with rotation.

The private keys of a published package open every Welcome addressed to it, so a compromise of the device reaches back as far as the oldest package whose keys it still holds. Two things limit that. The package states a lifetime, after which every validator rejects it. RFC 9420 [§7.2](https://www.rfc-editor.org/rfc/rfc9420.html#section-7.2) asks an application to set a maximum lifetime and reject a longer one; XMTP does not. The lifetime an installation publishes is 12 weeks by default, an installation may publish a longer one at its own risk, and no validator rejects a package for its length. And an installation that is online replaces its package: before `not_after`, and after it accepts a Welcome addressed to the current package. Neither replacement is a requirement, because an installation can be offline past its own expiry or have background work disabled, and the protocol keeps working: while all of an installation's packages are expired, it cannot be added to a group, and nothing else breaks.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| JOIN-003 | Reusable key packages | While a key package is the most recently published one for its installation, that installation MUST retain the private keys that open a Welcome addressed to it, however many Welcomes addressed to the same package it has processed. | Two senders adding the installation at the same time both address the same package. Keys destroyed after the first join lose the second. |
| JOIN-016 | Rotation changes keys | When an installation publishes a replacement key package, that package MUST carry an `init_key`, a leaf node `encryption_key`, and a wrapper public key generated for it, and MUST NOT reuse those of any earlier package. | A rotation that republishes the same keys moves the expiry date and leaves the compromise window unbounded. |

## 3. Destroying key material

Deleting a private key is the only thing that makes a Welcome already sent to it unreadable. Deleting early gives the shortest compromise window; deleting late keeps a Welcome that is still in flight openable. Publication of a replacement is confirmed when the backend's publish response carries a sequence id greater than 0 for the new package; API-221 owns the publish response and API-286 owns the sequence id range. From that confirmation, every superseded package is destroyed after a fixed delay, unless a Welcome the client has received is still waiting to be processed. A Welcome that is being held for a later attempt (JOIN-087, JOIN-084, JOIN-057, JOIN-094) delays that destruction until it is completed or rejected.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| JOIN-074 | Destroy superseded key material | When 1 day has passed since the backend's publish response for a later key package carried a sequence id greater than 0, and the client holds no Welcome it has received and not yet completed or rejected, the installation MUST destroy every private key of each superseded key package. For a package whose publish returned no sequence id, the day starts when a read of the installation's key package topic serves another package whose sequence id is at least the highest one in a publish response the installation holds. | The only value those keys still hold is to whoever takes the device. A publish whose response was lost may be the package senders use, so only that read shows it is not, and without it the keys are kept for ever. |
| JOIN-018 | Retain while work is pending | While an installation holds a received Welcome it has not completed or rejected, the installation MUST NOT destroy the private keys of any key package. | Destroying them turns a retriable join into a permanent loss of the conversation, and the client cannot ask for the Welcome again. |
| JOIN-019 | Destroy every part together | When an installation destroys a key package's private key material, it MUST destroy the private key of its `init_key`, of its leaf node `encryption_key`, and of the wrapper public key its `WelcomeWrapperEncryption` extension advertised. | Leaving any one of them behind leaves the Welcome readable and defeats the deletion. |
| JOIN-020 | Confirmation precedes destruction | An installation MUST NOT destroy a key package's private key material before the backend's publish response for a later key package has carried a sequence id greater than 0, except that it MAY destroy at once the material of a package whose own publish the backend failed with `INVALID_ARGUMENT`. | Destroying first and failing to publish leaves the installation with no package anyone can use, and no member can add it back. A refused package was never stored (API-224), so no sender can address it. |

## 4. Unwrapping a Welcome

A standard MLS Welcome is generated once for the whole set of members a commit adds. It is encrypted, but it carries publicly readable metadata that identifies which installations were added at the same time. XMTP adds a second layer of encryption for each recipient, so that a server operator cannot read that metadata, and uses a post-quantum ciphersuite in that outer layer so that traffic recorded now cannot be opened by a quantum computer later. Two wrappers exist, a classical one and the post-quantum one. Which one a sender used is recorded on the message, because a recipient can hold private keys for both and cannot tell them apart from the ciphertext.

The wrapper is not a security boundary against the sender; MLS decides what a Welcome may say. The wrapper decides who can read it at all. The choice belongs to the recipient, stated in its key package, and a sender may not substitute another.

```proto
message WelcomeMessage {
  message V1 {
    // Derives the topic.
    bytes installation_key = 1;
    // Encrypted MLS Welcome.
    bytes data = 2;
    // Unset when this message is the pointee of a WelcomePointer.
    bytes hpke_public_key = 3;
    xmtp.mls.message_contents.WelcomeWrapperAlgorithm wrapper_algorithm = 4;
    bytes welcome_metadata = 5;
  }

  message WelcomePointer {
    // Derives the topic.
    bytes installation_key = 1;
    // Encrypted WelcomePointer.
    bytes welcome_pointer = 2;
    bytes hpke_public_key = 3;
    xmtp.mls.message_contents.WelcomePointerWrapperAlgorithm wrapper_algorithm = 4;
  }

  oneof version {
    V1 v1 = 1;
    WelcomePointer welcome_pointer = 2;
  }
}
```

```proto
enum WelcomeWrapperAlgorithm {
  WELCOME_WRAPPER_ALGORITHM_UNSPECIFIED = 0;
  WELCOME_WRAPPER_ALGORITHM_CURVE25519 = 1;
  WELCOME_WRAPPER_ALGORITHM_XWING_MLKEM_768_DRAFT_6 = 2;
  // Only used for WelcomePointee's
  WELCOME_WRAPPER_ALGORITHM_SYMMETRIC_KEY = 3;
}
```

A Welcome with an unsupported wrapper algorithm, MLS version, or ciphersuite is held rather than rejected, because an upgrade can make it readable. A group-context extension absent from the receiving key package's advertised capabilities receives the same hold. The hold has a deadline so that an unreadable Welcome does not keep key material and queue capacity for ever. The adder is read from the ratchet tree, not from the envelope: anyone who reads a key package can wrap a Welcome for its installation, so the wrapper names the recipient and never the sender.

A Welcome can also name key material the client does not hold. JOIN-084 gives that Welcome a recovery window under the same unreadable hold; JOIN-063 prevents a change of failure reason or a restart from extending its deadline. A lookup that fails because of a database or lock error has not established that a key is absent and remains eligible for retry under JOIN-085.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| JOIN-071 | Wrap under the advertised key | When a sender wraps a Welcome or a welcome pointer for an installation, it MUST encrypt it under the `algorithm` and `pub_key` of the `WelcomeWrapperEncryption` extension of that installation's key package, or under `WELCOME_WRAPPER_ALGORITHM_CURVE25519` and the key package's `init_key` when the extension is absent, and MUST set `wrapper_algorithm` and `hpke_public_key` on the message to the values it used. | Any other choice produces a message the recipient cannot open, and the recipient cannot tell that from a corrupt one. |
| JOIN-076 | Unwrap under the advertised key only | When a client unwraps a Welcome, it MUST decrypt with the private key of the key package whose advertised public key equals the message's `hpke_public_key`, under the message's `wrapper_algorithm`, and MUST reject the Welcome when that algorithm is not the one that key package advertised. | Without the rejection, an attacker who can break the classical wrapper strips the post-quantum one by relabelling the message, and the recipient accepts the downgrade silently. |
| JOIN-087 | Hold an unreadable Welcome | When a Welcome cannot be read because its protobuf `WelcomeMessage.version` variant, `wrapper_algorithm`, MLS `version`, or MLS ciphersuite is unsupported by the client, or a group-context extension is absent from the receiving KeyPackage leaf node's advertised capabilities, the client MUST keep the Welcome for 1 day from the time it first encountered that unsupported value, and MUST record a terminal rejection at the first attempt after that day on which it still cannot read it. | No deadline leaves an unreadable Welcome holding key material and queue capacity for ever. |
| JOIN-084 | Hold a Welcome with missing keys | When a lookup needed to read a Welcome or welcome pointer completes and finds no key-package reference for its `hpke_public_key`, no key package for that reference, or no post-quantum private key for that package, the client MUST apply JOIN-087's retention and terminal-rejection policy, measuring the hold from the first such result unless an unreadable hold already exists. Before terminal rejection, the client MUST retry with its current keys and MUST NOT reject solely because the hold expired if it can now read the Welcome. | A sender can name keys the installation never held; retaining those Welcomes forever prevents legitimate joins and key retirement. |
| JOIN-063 | A deadline is set once | When a client retries an envelope it is holding to a deadline, it MUST NOT move that deadline later. | A deadline refreshed on every attempt never arrives. |
| JOIN-025 | The adder comes from the ratchet tree | When a client records the inbox and installation that added it to a group, it MUST take them from the credential and `signature_key` of the leaf node at `GroupInfo.signer` ([RFC 9420 §12.4.3.1](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.4.3.1)) in the ratchet tree, and MUST NOT take them from any field outside the Welcome's authenticated payload. | An adder read from an unauthenticated field is one the wrapper's author chose. |
| JOIN-026 | Reject a non-Welcome payload | When the plaintext behind `data` is not an `MLSMessage` whose `wire_format` is `mls_welcome` ([RFC 9420 §6](https://www.rfc-editor.org/rfc/rfc9420.html#section-6)), the client MUST record a terminal rejection for it. | |
| JOIN-027 | Refuse a pre-shared key | When a Welcome's `GroupSecrets.psks` ([RFC 9420 §12.4.3.1](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.4.3.1)) is not empty, the client MUST record a terminal rejection for it. | The client establishes no pre-shared keys, so it cannot derive the group secrets. |

## 5. Welcome pointers

A commit that adds many installations produces one Welcome body repeated for each of them. A welcome pointer splits that: the body is encrypted once under a fresh symmetric key and published to a randomly generated topic, and a small message carrying that topic and the key is encrypted to each recipient. A sender uses the pointer form when more than two of the installations a commit adds advertise both `WELCOME_POINTEE_ENCRYPTION_AEAD_TYPE_CHACHA20_POLY1305` and the `WELCOME_WRAPPER_ALGORITHM_XWING_MLKEM_768_DRAFT_6` wrapper; the others receive an inline Welcome in the same publish.

The pointee is published as a `WelcomeMessage.V1` whose `installation_key` is the pointer's `destination`, so it is stored on the welcome topic for that identifier. TOPIC-001 owns the topic layout and TOPIC-002 owns identifier length validation. The pointer is the only thing that carries the topic. That is what keeps the pointee unlinkable to the group and to its recipients, and it is why JOIN-029 forbids deriving `destination` from anything an observer already has.

The lowest sequence id at a topic is a meaningful rule because API-201 owns ordered visibility and reads, API-288 keeps assigned sequence ids stable, and API-289 orders later publishes after visible envelopes.

```proto
message WelcomePointer {
  message WelcomeV1Pointer {
    // The topic of the welcome message. For V1, this means that it will be the first message in the topic, so no other identifier is required
    bytes destination = 1;
    // The algorithm used to encrypt the welcome pointer
    WelcomePointeeEncryptionAeadType aead_type = 2;
    // The encryption key of the welcome message. Must match key size specified by the aead_type.
    bytes encryption_key = 3;
    // Nonce used to encrypt the data field. Must match nonce size specified by the aead_type.
    bytes data_nonce = 4;
    // Nonce used to encrypt the welcome_metadata field. Must match nonce size specified by the aead_type.
    bytes welcome_metadata_nonce = 5;
  }

  oneof version {
    WelcomeV1Pointer welcome_v1_pointer = 1;
  }
}

enum WelcomePointeeEncryptionAeadType {
  WELCOME_POINTEE_ENCRYPTION_AEAD_TYPE_UNSPECIFIED = 0;
  // Use same encoding as openmls::AeadType
  WELCOME_POINTEE_ENCRYPTION_AEAD_TYPE_CHACHA20_POLY1305 = 3;
}

// MUST match the WelcomeWrapperAlgorithm enum values without 25519 so that the i32 transformations are compatible
enum WelcomePointerWrapperAlgorithm {
  WELCOME_POINTER_WRAPPER_ALGORITHM_UNSPECIFIED = 0;
  WELCOME_POINTER_WRAPPER_ALGORITHM_XWING_MLKEM_768_DRAFT_6 = 2;
}
```

`WelcomePointerWrapperAlgorithm` admits no classical value: a welcome pointer is always wrapped under `WELCOME_WRAPPER_ALGORITHM_XWING_MLKEM_768_DRAFT_6`. The pointee advertisement and the wrapper advertisement are independent, so a sender checks both before it chooses the pointer form (JOIN-091).

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| JOIN-091 | Pointers only to pointer-capable installations | When a commit adds installations, the sender MUST send a welcome pointer only to an installation whose key package advertises `WELCOME_POINTEE_ENCRYPTION_AEAD_TYPE_CHACHA20_POLY1305` in its `0xff04` extension and `WELCOME_WRAPPER_ALGORITHM_XWING_MLKEM_768_DRAFT_6` in its `0xff03` extension, and MUST send an inline Welcome to every other installation the commit adds. | A recipient sent a form it did not advertise loses the conversation while everyone else joins. `WelcomePointerWrapperAlgorithm` has no classical value, so a pointer wrapped for a classical-only installation cannot be decoded. |
| JOIN-029 | Randomly generated pointee topic | A sender MUST set a pointee's `destination` to 32 bytes drawn from a cryptographically secure random source for that pointee alone, and MUST NOT derive them from the group, the commit, or any recipient. | The topic is the pointee's only access control. Deriving it from the group tells an observer who was added to what. |
| JOIN-030 | Fresh key for each pointee | A sender MUST set `encryption_key` to 32 bytes drawn from a cryptographically secure random source for each pointee it publishes. | One key across pointees lets a recipient of either read both. |
| JOIN-065 | Distinct nonce per value | A sender MUST set `data_nonce` and `welcome_metadata_nonce` to two different 12-byte values. | A nonce reused under one key with ChaCha20-Poly1305 discloses both plaintexts and forfeits the authentication. |
| JOIN-078 | ChaCha20-Poly1305 pointee encryption | A sender MUST encrypt a pointee's `data` and `welcome_metadata` with ChaCha20-Poly1305 under `encryption_key` and the matching nonce, and MUST set `aead_type` to `WELCOME_POINTEE_ENCRYPTION_AEAD_TYPE_CHACHA20_POLY1305`. | The pointee sits on a topic anyone may publish to. Without authentication a third party substitutes a Welcome of its own and the recipient cannot tell. |
| JOIN-064 | Reject an unauthentic pointee | When a pointee's `data` or `welcome_metadata` fails ChaCha20-Poly1305 authentication under the pointer's `encryption_key` and nonce, the client MUST reject it. | |
| JOIN-032 | Take only the first pointee | When a client resolves a welcome pointer, it MUST use the message with the lowest sequence id on the topic `destination` names and MUST NOT use any later one. | Anyone who learns the topic can publish after the sender. Accepting a later message lets the race decide which group the recipient joins. |
| JOIN-056 | Check the pointee's form | When a client resolves a welcome pointer, it MUST reject a pointee whose `installation_key` is not the pointer's `destination` or whose `wrapper_algorithm` is not `WELCOME_WRAPPER_ALGORITHM_SYMMETRIC_KEY`. | Authentication alone says that someone with the key wrote the bytes. The form check stops a Welcome meant for a different recipient being replayed into this one's pointee topic. |
| JOIN-033 | No pointer chains | If the message a welcome pointer resolves to is itself a `WelcomePointer`, then the client MUST record a terminal rejection for the pointer. | Following a chain lets one message send a client round an unbounded number of fetches. |
| JOIN-094 | Bounded resolution of well-formed pointers | While the pointee of a welcome pointer whose `destination` is 32 bytes, whose `aead_type` is `WELCOME_POINTEE_ENCRYPTION_AEAD_TYPE_CHACHA20_POLY1305`, whose `encryption_key` is 32 bytes, and whose `data_nonce` and `welcome_metadata_nonce` are two different 12-byte values cannot be retrieved, the client MUST keep the pointer for a later attempt until the deadline of an unreadable hold already established under JOIN-087 or JOIN-084, or until 3 days after the backend's timestamp on the pointer when no such hold exists. At the first attempt at or after that deadline, the client MUST record a terminal rejection if that attempt has not retrieved the pointee. | A pointer to nothing costs the sender one message and the recipient unlimited fetches; a separate unreadable hold lets recovered keys or format support complete a join. A pointer of any other form names no welcome topic under TOPIC-002 or can never open a pointee under JOIN-078, so it falls under JOIN-048 and is not held. |
| JOIN-035 | Unresolved is not joined | When a client records a terminal rejection for a welcome pointer under JOIN-094, it MUST NOT install any group state for it. | The client learned nothing about the group, so state installed for it is state no member can reconcile. |

## 6. The join anchor

A Welcome hands over a group that already has history. Without a stated position in that history the joiner has two bad choices: read the whole topic and fail to decrypt everything before it was added, or read nothing and miss messages sent between the commit and its first poll. The sender therefore sets `message_cursor` to the sequence id, on the group's message topic, of the commit that added the recipient. MLS requires that commit to exist before the Welcome derived from it ([RFC 9420 §12.4.3](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.4.3)). GMOD-018 requires the sender to read back and apply the commit before publishing its Welcome, and API-289 orders that later publish after the visible commit. Thus the anchor is less than the Welcome's own sequence id or the Welcome is invalid. A client that installs a group reads its message topic from the anchor; reading earlier produces only messages it holds no secrets for.

The anchor is encrypted with the Welcome and opened under the same key, so it shares the Welcome's trust assumptions: a client that reads a Welcome trusts the sender for that Welcome's contents, and the anchor is one of them. The client bounds the anchor under JOIN-037 and JOIN-038 and does not otherwise verify it. A Welcome that waits for its group's message topic to reach the anchor is rejected when that topic, read 3 days after the Welcome, still ends below the anchor and the client has processed it to that end (JOIN-092). It is also the load-bearing input to the replacement rules in section 7, because it is the only statement in a Welcome about where the group's message topic stood.

```proto
// Encrypted with the welcome message data.
message WelcomeMetadata {
  uint64 message_cursor = 1;
}
```

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| JOIN-036 | Every join names an anchor | When a Welcome for a group whose conversation type is not one-shot carries no `WelcomeMetadata` as defined above, the client MUST record a terminal rejection for that Welcome. | An anchor supplied outside the payload is chosen by whoever relays the message. |
| JOIN-037 | The anchor precedes the Welcome | If a Welcome's `message_cursor` is not less than the Welcome's own sequence id, then the client MUST record a terminal rejection for it. | |
| JOIN-038 | New groups start empty | If a Welcome's `message_cursor` is 0 and the group's `GroupContext.epoch` is not 0, then the client MUST record a terminal rejection for it. | A `message_cursor` of 0 for a group with history is an instruction to re-read the whole topic. |
| JOIN-066 | Pre-join messages are not missed | A client MUST NOT report a message whose sequence id is less than its join anchor as one it failed to receive or decrypt. | Counting messages the joiner holds no secrets for as failures reports a fork that does not exist. |

## 7. Replacing local group state

Being added to a group a client already holds is normal: a member is removed and added back, or two devices are added by different commits. A Welcome for a group already present is therefore not an error, but installing it discards the epoch secrets the client holds, and a client that discards live secrets leaves the group in a broken state.

A Welcome replaces existing state only when the client can see that the existing state has ended. Publication order proves nothing, because the backend orders Welcomes by arrival and MLS orders groups by epoch; the two orders are independent. The decision rests on the anchor and the epoch together, compared against the group's stored cursor on its message topic and the client's MLS state for the group. A Welcome whose anchor equals the stored cursor is decided by JOIN-042, because that is where a re-add lands.

PROC-005 owns the ordered processing position `P` used here as the stored cursor. PROC-008 owns durability of that position with the effects of processing.

A group restored from an archive (ARCH-015) is a placeholder: it holds messages and metadata but no MLS state from a join, and the epoch of the placeholder group is generated locally. The rules above are about live state that must not be discarded. A placeholder has none, so a Welcome for it is validated and installed as a first join, and the messages the import wrote are kept under JOIN-044. JOIN-042 keeps guarding a group that was joined through MLS; the absence of archived secrets does not satisfy its removal-commit condition.

A held group's conversation type and dm id do not change through a Welcome. A placeholder keeps those its archive declared under ARCH-020, and a live group keeps those it was created or first joined with. JOIN-089 rejects a Welcome that would change either, for a placeholder and a live group alike, before the cursor and epoch comparisons below and before JOIN-092 keeps the Welcome for a later attempt.

```mermaid
flowchart TD
  W[Welcome for a group already held] --> K{Same conversation type<br/>and dm id}
  K -->|no| R[Identity changed: reject]
  K -->|yes| P{Stored cursor<br/>versus message_cursor}
  P -->|cursor greater| X[Already processed: reject]
  P -->|cursor less, group active| E{Incoming epoch<br/>greater than the local epoch}
  E -->|yes| Y[Process the topic up to the anchor first]
  E -->|no| X
  P -->|cursor not greater, group inactive| Z{Incoming epoch<br/>not less than the local epoch}
  Z -->|yes| I[Replace]
  Z -->|no| X
```

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| JOIN-092 | Bounded wait for the removal | When a Welcome names a group the client already holds whose MLS state is active, whose stored MLS epoch is less than the Welcome's `GroupContext.epoch`, and whose stored cursor on its message topic is less than `message_cursor`, the client MUST keep the Welcome for a later attempt and MUST NOT replace the state before it has processed that topic up to `message_cursor`. If the newest sequence id on that topic, read no earlier than 3 days after the backend's timestamp on the Welcome, is less than `message_cursor`, then once the client has processed the topic through that sequence id it MUST record a terminal rejection for the Welcome and MUST NOT alter the group's state. | The commit that removed this installation lies in that range, and until it is processed the client cannot tell a re-add from an attempt to displace a live group. GMOD-018 has the sender read back its commit before it publishes the Welcome, so a topic that still ends below `message_cursor` 3 days later never reaches it, and without a deadline the Welcome and the key material JOIN-018 retains for it are held for ever. |
| JOIN-042 | Replace only an ended group | A client MUST replace existing group state from a Welcome only when its MLS state for the group is inactive, because it has applied a commit that removed this installation, and the Welcome's `GroupContext.epoch` is not less than the existing state's epoch. When the Welcome's `message_cursor` equals the group's stored cursor on its message topic, the client MUST also require the Welcome's `GroupContext.epoch` and `tree_hash` to equal those of the existing state, and otherwise MUST record a terminal rejection for it. | An earlier epoch is always stale, and an inactive group has no secrets left to lose. At the stored cursor the only valid Welcome comes from the commit that removed this installation, and the client already holds that commit's tree. The Welcome's GroupInfo signer must be a leaf of that tree, which a former member is not. A removed installation keeps that commit's epoch and tree hash but not its confirmed transcript hash. |
| JOIN-043 | Discard a superseded Welcome | If a Welcome's `message_cursor` is less than the group's stored cursor on its message topic, then the client MUST record a terminal rejection for it and MUST NOT alter the group's state. | The client holds everything such a Welcome describes and more, so installing it would move the group backwards. |
| JOIN-044 | Preserve history across a rejoin | When a client replaces group state from a Welcome, it MUST preserve the messages it already holds for that group. | A user removed and added back still owns what they read. |
| JOIN-045 | Join exactly once per Welcome | A client MUST apply the effects of a given Welcome to its local state at most once, whatever the order and repetition in which the Welcome is delivered. | A second application inserts the join twice and moves the stored cursor backwards. |
| JOIN-046 | Install atomically | After any interruption of a join, a client MUST hold either no state for the group or state whose stored cursor on the message topic equals the `message_cursor` of the Welcome it installed. | A group whose cursor disagrees with its secrets is indistinguishable from a fork, and a retry cannot repair it. |
| JOIN-085 | Retry other local failures | If a Welcome fails for a reason that is not a property of the Welcome itself, other than unsupported input under JOIN-087, missing key material under JOIN-084, or unresolved welcome-pointer retrieval under JOIN-094, then the client MUST leave it eligible for a later attempt and MUST NOT record a terminal rejection. | Rejecting a Welcome because a local operation failed loses a join that retrying that operation could complete. |
| JOIN-048 | Reject invalid input finally | When a Welcome fails for a reason that a later attempt cannot change, the client MUST record a terminal rejection for it. | A Welcome queue that never drains stops every valid Welcome behind it. |
| JOIN-080 | A Welcome activates a restored group | When a Welcome names a group the client holds only as a restored placeholder under ARCH-015, the client MUST validate it under sections 4, 6, 8, and 9 as a first join, MUST NOT compare its `GroupContext.epoch` with the placeholder's epoch or reject it under JOIN-092 or JOIN-042 on account of the placeholder, and MUST install its state with the stored cursor at `message_cursor`. It MUST preserve the imported messages under JOIN-044. | The placeholder's epoch is generated locally and says nothing about the group, so a comparison against it rejects the only Welcome that can activate the conversation. |
| JOIN-089 | A Welcome keeps the conversation identity | When a Welcome names a group the client holds, including a restored placeholder under ARCH-015 and a group whose MLS state is active, the client MUST record a terminal rejection for it unless its `CONVERSATION_TYPE` equals the stored conversation type and, when that type is DM, the dm id derived from its `DM_MEMBERS` under DMS-001 equals the stored dm id. It MUST decide this before JOIN-092 applies, without processing the group's topic. | A Welcome that reuses a held group id would otherwise put a conversation of another kind or pair under the id, history, and label the app already shows, and the user's messages would go to that conversation's members. No message on the topic can make a different kind or pair match, so waiting for one only holds the Welcome. |

## 8. Validating the asserted membership

A Welcome tells the joiner who is in the group. That statement comes from the sender: the ratchet tree in the `GroupInfo`'s `ratchet_tree` extension ([RFC 9420 §12.4.3.3](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.4.3.3)) lists every leaf node, and each leaf node's credential names an inbox. The joiner has no prior state to check that list against. What it has is the membership component in the group context: the `GROUP_MEMBERSHIP` component whose identifier and map encoding [META section 2](META-group-metadata.md#2-well-known-components) fixes, and whose entries GMOD-005 encodes as a `GroupMembershipEntry` naming, for each inbox in the group, the identity `sequence_id` at which that inbox's installations are to be checked and the `failed_installations` a sender could not add. In this section, `members` means that map and `failed_installations` means the entry's list. GMOD-006 and GMOD-009 own the sender's resolved identity reference and installation accounting. IDENT-070 and IDENT-071 own exact association state and its validation by the client.

The check runs in both directions. Every leaf node's `signature_key` must be an installation key the named identity state associates with the leaf's inbox, or an attacker adds a leaf under a victim's inbox id. Every installation the named identity state associates must be a leaf node or be listed in `failed_installations`, or an attacker silently excludes a victim's other devices and the group never notices they are missing. A joiner resolves the referenced identity state itself: when it does not hold an inbox's association state at the referenced sequence id, it fetches that inbox's identity updates and retries the fetch for a period the implementation chooses before it treats the reference as one that does not exist.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| JOIN-040 | Membership travels with the Welcome | A sender MUST include the group's ratchet tree in the Welcome's `GroupInfo` as a `ratchet_tree` extension ([RFC 9420 §12.4.3.3](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.4.3.3)). | A Welcome that leaves the joiner to fetch the tree elsewhere lets whoever answers that fetch decide who the joiner believes is in the group. |
| JOIN-049 | Membership names exact identity state | When a Welcome's `GroupContext` carries no `GROUP_MEMBERSHIP` component, or its `members` map has no entry for the inbox named by a leaf node in the ratchet tree, the client MUST record a terminal rejection for that Welcome. | Without an exact reference the joiner checks against whatever it happens to hold, so two joiners reach different answers about the same group. |
| JOIN-050 | Bound the identity references | If any value in `members` is 0 or is not less than the Welcome's own sequence id, then the client MUST record a terminal rejection for it. | A reference at or after the Welcome can never be resolved, and 0 asserts nothing. |
| JOIN-051 | Resolve before deciding | When a client does not hold an inbox's association state at the sequence id `members` names, it MUST fetch that inbox's identity updates through that sequence id before it decides on the membership, and MUST NOT accept or reject the membership on the state it happens to hold. | Deciding on partial state makes the outcome depend on what this device had cached, so two devices of the same inbox disagree about whether the group is valid. |
| JOIN-052 | Every leaf is accounted for | When any leaf node's `signature_key` is not an installation key that the association state of the leaf's inbox, at the sequence id `members` names for it, associates with that inbox, the client MUST record a terminal rejection for the Welcome. | This is the check that stops one member from placing a leaf under another member's identity. |
| JOIN-053 | Expected installations are present | When an installation key that an inbox's association state, at the sequence id `members` names for it, associates with that inbox is neither the `signature_key` of a leaf node nor listed in that inbox's entry `failed_installations`, the client MUST record a terminal rejection for the Welcome. | Without it a sender quietly leaves a member's other devices out, and the group's own record shows nothing missing. |
| JOIN-095 | An own adder is a current installation | When the adder under JOIN-025 names the own inbox and its `signature_key` is not an installation key of the newest association state of the own inbox that the client holds, the client MUST record a terminal rejection for the Welcome. | JOIN-052 checks the adder at the state the sender names, which can predate a revocation. An own adder makes the group a sync group under SYNC-010, so a revoked device would otherwise read every consent decision and the HMAC root key. |
| JOIN-054 | Decide on unchanging evidence | When a client accepts a Welcome, it MUST install the same membership it validated, and MUST NOT install one re-derived after the validation. | Validation reads identity state over the network, and the membership can be re-derived in between. |
| JOIN-059 | Unresolvable references are terminal | When a client has fetched an inbox's identity updates and the backend holds no update at the sequence id `members` names for it, the client MUST record a terminal rejection for the Welcome. | A reference that names a state never published can never become valid, and the Welcome would otherwise stay in the queue for ever. |

## 9. Validating the group at join time

The last check is on the group itself. A joiner is being handed a configuration it did not choose, and some configurations are not safe to accept: a conversation that claims to be between two people but whose policies let a third be added, or a group whose rules require behaviour this client does not have. Refusing at join is the only chance to refuse at all, because after the join the client is a member and the configuration is the group's.

DMS-021 owns the checks for a two-party conversation under JOIN-060, including a Welcome from the joiner's own inbox. [META section 2](META-group-metadata.md#2-well-known-components) owns the minimum client version component. [CONS section 3](CONS-consent.md#3-what-the-client-writes) owns what a join stores, which is nothing, and CONS-060 and CONS-031 own consent filtering. SYNC-010 owns the checks for a sync-group Welcome under JOIN-060. GMOD-040 owns the group root; JOIN-083 checks that the root a Welcome carries names its group.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| JOIN-060 | Reject an inconsistent group | When the group a Welcome carries does not satisfy every property its stated conversation kind requires, the client MUST record a terminal rejection for the Welcome, whatever privileges that kind carries. | The kinds carrying the most privilege are the ones an attacker names, so a kind exempted from the check is a kind anyone can claim. |
| JOIN-057 | Refuse an unsupported group | When the group a Welcome carries states a minimum client version later than this client's, the client MUST NOT join and MUST leave the Welcome eligible for a later attempt. | Joining a group whose rules the client cannot follow forks it. An upgrade makes the Welcome valid, which no other rejection reason does. |
| JOIN-058 | The joiner is present | When a client joins a group from a Welcome, it MUST establish that one leaf node's `signature_key` is its own installation key, and MUST record a terminal rejection otherwise. | A Welcome the client can open but that adds somebody else installs a group the client can never send in and whose commits it can never apply. |
| JOIN-083 | The root names the group | When a Welcome's group context has no `GROUP_ROOT_KEY` component of 32 bytes, or the public key of the group signing key derived from it under GMOD-041 is not the Welcome's `group_id`, the client MUST record a terminal rejection for the Welcome. | A joiner without the root cannot publish to the group, and a restored conversation activated by a group whose creator lacks the root continues into a stranger's group. |

## Known limitations

A client that has read a group's messages past a Welcome's anchor cannot distinguish a re-add it has already applied from a re-add whose commit it read but whose Welcome arrived late. It rejects both under JOIN-043. The cost is a missed rejoin in a rare ordering; accepting instead would let a stale Welcome displace a live group, which is worse.

A sender cannot tell whether an installation has destroyed a key package's private keys under JOIN-074, only that a replacement was published. A Welcome addressed to a superseded package more than 1 day after the replacement is wrapped for keys that may be gone, and the recipient sees only an unwrap failure.

A publish that lands after the settlement read, because another process sharing the database rotates at the same moment or the backend commits a request after its client gave up, can be the served package while its keys are destroyed 1 day later. Adds to the installation fail until the next rotation.

The client does not validate the `Lifetime` of the leaf nodes in the ratchet tree of a group it joins. RFC 9420 §7.3 makes that check RECOMMENDED for a received tree, and XMTP omits it, so an expired leaf that a sender would reject under JOIN-008 is accepted inside a Welcome. No change is planned.

The window in JOIN-094 makes a welcome pointer that never resolves indistinguishable from one whose pointee is merely slow. A recipient on a long network outage can give up on a valid join, and no later event tells it that it did.

A Welcome held for a later attempt under JOIN-087, JOIN-084, or JOIN-057 delays the destruction of every superseded key package under JOIN-074 until the held Welcome is completed or rejected.

A flood of held Welcomes makes PROC-020 reject the oldest held Welcome before its JOIN deadline. A valid Welcome held under JOIN-057, JOIN-084, JOIN-087, JOIN-092, or JOIN-094 can be lost this way, and the installation needs a new Welcome to join.

JOIN-092 trusts the newest sequence id that a read returns 3 days after the Welcome. A lagging copy of the store can answer that read (API-201 permits one). If that copy has not replayed the re-add commit, the client rejects a valid re-add and needs a new Welcome to rejoin.

The deadline in JOIN-092 does not bound a Welcome whose group topic holds `message_cursor` but whose processing stops below it. That Welcome, and the key material it retains, stays held until that processing resumes or the group ends.

An upgrade after terminal rejection under JOIN-087 cannot recover that Welcome; the installation needs a new Welcome to join. Unknown protobuf fields already stripped from pending rows by an older client cannot be reconstructed locally. Upgrading preserves the existing storage format but does not refetch those rows; if the stripped fields are needed to read the Welcome, the installation needs a new Welcome to join.

The existing upgrade limitation now states that pre-fix rows with stripped fields are not repaired. No schema or data migration is needed: the blob remains a serialized ServerEnvelope (`crates/xmtp_db/src/encrypted_store/incoming_envelope.rs:80`, `crates/xmtp_mls/src/groups/welcome_sync.rs:339`).

The missing-key window in JOIN-084 cannot distinguish keys that will be restored later from keys the installation never held. Restoring keys after terminal rejection does not recover that Welcome; the installation needs a new Welcome to join.

JOIN-083 binds a Welcome to the group root, not to one MLS group. Every installation that was ever a member holds the root and can create a second MLS group with the same group id and root. Its Welcome passes JOIN-083 at an installation that does not hold the group, and activates a restored conversation with that id when its conversation type and dm id match (JOIN-080, JOIN-089). JOIN-042 keeps it from replacing a group whose MLS state is active, and JOIN-089 keeps it from changing a held group's conversation type or dm id. JOIN-042 also keeps it from replacing an inactive group at the stored cursor, because there it needs the epoch and tree hash of the commit that removed this installation. Nothing protects a group whose MLS state is inactive because a commit removed this installation from a Welcome whose `message_cursor` is above the stored cursor: a Welcome with the same conversation type and dm id replaces it when its epoch is not less than the local one, and the creator of the second MLS group chooses that epoch, its members, and its `message_cursor`. The conversation keeps the consent the user gave the earlier group (CONS-058). A DM built this way reads its builder's inbox record, never the group's (CONS-052). The re-added installation then sends to members that creator chose, and the member list can look unchanged because the creator can add any installation whose key package the backend serves. The chosen `message_cursor` becomes the stored cursor, so pending envelopes at or below it are discarded and a later Welcome whose `message_cursor` is lower is rejected under JOIN-043. The replaced state is active, so under JOIN-042 no later Welcome replaces it until the creator's MLS group removes this installation. Closing this needs a re-add Welcome that a former member cannot produce, which is a protocol change no spec states yet.

JOIN-095 reads the own identity state this device holds. A device that has not yet loaded a revocation still accepts a Welcome from the revoked installation. A Welcome that an own installation sent before its revocation is refused when this device processes it afterwards. The decision is local and does not change the group.
