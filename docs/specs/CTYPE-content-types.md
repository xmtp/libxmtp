---
prefix: CTYPE
status: draft
---
# Content types

How a message payload declares what it is, so that a client which does not understand a type can still show something useful rather than nothing.

Every application message a client publishes carries an `EncodedContent`: a content type identifier, the content bytes, the parameters a decoder needs, and a fallback text. The identifier names an authority, a type, and a version. A codec is the pair of functions that turn a value into an `EncodedContent` and back for one identifier. This spec owns the identifier scheme, the envelope, what a client does with a type it cannot decode, and the catalogue of standard types with their encodings.

```mermaid
flowchart LR
  A[App value] -->|codec.encode| E[EncodedContent<br/>type, parameters, fallback, content]
  E -->|MLS application message| N[(The backend)]
  N --> R[Recipient client]
  R -->|codec for authority, type, major| D[Decoded value]
  R -->|no codec, or decode fails| F[fallback text and the raw EncodedContent]
```

## Scope

In scope: the content type identifier and how a client matches one; the `EncodedContent` envelope and its parameters, fallback, and compression fields; the codec contract and the errors it reports; the push flag a type carries; content nested inside content; coexistence of a legacy and a current version of a type; what a client does with a type it cannot decode; and the catalogue of standard types with their schemas.

Out of scope: the `PlaintextEnvelope` that carries an `EncodedContent` inside an MLS message, the message id, and publishing (`?SEND`); receipt, ordering, and storage of messages (`?PROC`); the group metadata a commit changes and the transcript message a client derives from it (`?GMOD`, `?META`); the device sync payload (`?SYNC`); what a push server does with the push flag (`?PUSH`); the archive that carries stored content between installations (ARCH); and the effect of a delete or a leave request on group state (`?PROC`, `?GMOD`).

| Related | Relation |
| --- | --- |
| `?SEND` | Owns the `PlaintextEnvelope`, the idempotency key, and the send options a client publishes with. This spec owns the `EncodedContent` inside it and the push value a type supplies. |
| `?PROC` | Owns storing a received message and applying a `deleteMessage` to a stored message. This spec owns the type's schema and which types are deletable. |
| `?PUSH` | Owns the publish field that tells the backend whether to notify. This spec owns the value a type supplies for it. |
| `?SYNC` | Owns the sync message, an `EncodedContent` of its own type whose schema it states. |
| `?GMOD` | Owns the commit that a `group_updated` message summarises and the effect of a `leave_request`. |
| ARCH | Carries a stored message's `EncodedContent` bytes unchanged (ARCH-008), so a type a client does not decode survives export and import. |

## Terms

| Term | Meaning |
| --- | --- |
| Content type identifier | A `ContentTypeId`: `authority_id`, `type_id`, `version_major`, and `version_minor`. |
| Human-readable form | The string `authority_id/type_id:version_major.version_minor` of an identifier, for example `xmtp.org/text:1.0`. It appears on the wire only where a catalogue schema names it. |
| Encoded content | An `EncodedContent` as defined in section 2. |
| Codec | The encode and decode functions for one identifier, with the push value in section 4. A client ships the codecs of the catalogue; an app registers codecs for its own types. |
| Standard type | A type under authority `xmtp.org`, or one of the two `coinbase.com` types, in the catalogue in section 7. |
| Custom type | A type whose codec an app registers and the catalogue does not list. |
| Fallback | The `fallback` string of an encoded content: text a client shows when it cannot decode the content. |
| Nested content | An `EncodedContent` carried inside the `content` of another, as a reply carries the content it replies with. |
| Push value | The boolean a codec supplies for a type, which the client publishes as the push flag `?PUSH` owns. |

## 1. The identifier

An identifier has three parts that name the type and one that does not. The authority is the party that defines the type, named by a DNS name. The type is a name unique under that authority. The major version separates encodings that do not decode one another. The minor version marks an additive change: content under a later minor version decodes under a codec for an earlier one, so a client matches a codec without it.

`xmtp.org` is the authority of the standard types, and the catalogue in section 7 is the whole set. A type under `xmtp.org` that the catalogue does not list is one no client on the network can decode, and an app that publishes one takes a name a later standard type may need.

```proto
message ContentTypeId {
  string authority_id = 1; // authority governing this content type
  string type_id = 2; // type identifier
  uint32 version_major = 3; // major version of the type
  uint32 version_minor = 4; // minor version of the type
}
```

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| CTYPE-001 | Match on three values | When the client selects a codec for an encoded content, it MUST select the codec whose `authority_id`, `type_id`, and `version_major` equal the content's, and MUST NOT require `version_minor` to be equal. | A codec keyed on the minor version rejects every additive change, and a match that ignores the authority decodes one authority's type with another's codec. |
| CTYPE-002 | Standard authority is reserved | An app SHOULD NOT register a codec whose `authority_id` is `xmtp.org` for a `type_id` the catalogue in section 7 does not list. | |

## 2. The envelope

An `EncodedContent` carries the identifier, the parameters a decoder needs beyond the bytes, a fallback, an optional compression, and the content. A recipient reads `parameters` as the catalogue schema for the type says; a parameter the schema does not name is ignored. The fallback is the one part of the envelope every client reads, whatever the type, so a type that is not plain text carries one. Text and markdown carry none because their content is the text.

`compression` names an algorithm applied to `content` before encoding. `COMPRESSION_DEFLATE` is 0, so absent and deflate are told apart by presence alone. No client on the network inflates `content` before decoding it (Known limitations), so a sender that compresses produces content no recipient decodes.

```proto
// Recognized compression algorithms
enum Compression {
  COMPRESSION_DEFLATE = 0;
  COMPRESSION_GZIP = 1;
}

// EncodedContent bundles the content with metadata identifying its type
// and parameters required for correct decoding and presentation of the content.
message EncodedContent {
  // content type identifier used to match the payload with
  // the correct decoding machinery
  ContentTypeId type = 1;
  // optional encoding parameters required to correctly decode the content
  map<string, string> parameters = 2;
  // optional fallback description of the content that can be used in case
  // the client cannot decode or render the content
  optional string fallback = 3;
  // optional compression; the value indicates algorithm used to
  // compress the encoded content bytes
  optional Compression compression = 5;
  // encoded content itself
  bytes content = 4;
}
```

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| CTYPE-003 | Every message names its type | When the client publishes an application message, it MUST set `EncodedContent.type` and MUST NOT publish one whose `type` is absent. | A message with no type is stored by every recipient as content it cannot decode, with no fallback to show. |
| CTYPE-004 | Fallback for non-text types | When the client encodes content of a catalogue type whose Fallback column in section 7 is `yes`, it MUST set `fallback` to a non-empty string. | A recipient without that codec shows nothing for the message. |
| CTYPE-005 | Custom types carry a fallback | An app SHOULD set `fallback` on every content it encodes under a custom type it registers. | |
| CTYPE-006 | No compression | When the client publishes an application message, it MUST NOT set `compression`. | |

## 3. Codecs and undecodable content

A codec turns a value into an `EncodedContent` and back. What the encode function writes is the contract with every other client: the identifier, the content bytes in the encoding the catalogue states, the parameters, and the fallback. A codec that writes a different encoding under the same identifier splits the network at that type.

A client meets content it cannot decode as a matter of course: a custom type from an app it is not, a standard type from a newer client, or bytes a buggy sender produced. The message is still a message: it has an id, a sender, a position in the conversation, and it may be the target of a reply, a reaction, or a deletion. The client keeps it and hands the app what it has, which is the identifier, the fallback, and the raw envelope. Dropping it would make the conversation differ between a client that has the codec and one that does not.

An SDK reports four failures as distinct kinds: no codec is registered for the identifier, a codec's decode failed, a codec's encode failed, and the bytes are not an `EncodedContent` or carry no `type`.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| CTYPE-007 | Encode round trip | When a codec encodes a value, the result MUST carry `type` equal to the codec's identifier, and decoding that result with the same codec MUST return a value equal to the input. | |
| CTYPE-008 | Undecodable content is kept | When a received application message carries a `type` the client has no codec for, or its codec fails to decode it, or its bytes are not an `EncodedContent`, the client MUST store the message with its bytes unchanged and MUST expose to the app the message's identifier, its `fallback`, and the raw `EncodedContent` where the bytes decode as one. | A message dropped on one installation and kept on another leaves the two with different conversations. |
| CTYPE-009 | Failures are distinguishable | An SDK MUST let an app distinguish, as distinct error kinds, no codec registered for an identifier, a decode failure, an encode failure, and bytes that are not an `EncodedContent` or carry no `type`. | |

## 4. The push value

A type says whether a message of that type is worth a notification. A reaction, a read receipt, a deletion, and a group transcript are not; text and attachments are. The codec supplies the value and the client publishes it as the push flag on the message. `?PUSH` is expected to own that flag on the publish request and to require that the backend delivers a push notification only for a message whose flag is set. `?SEND` is expected to own the send options and to require that an app's explicit push option, when given, replaces the codec's value.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| CTYPE-010 | Push value from the catalogue | When the client publishes content of a catalogue type and the app gave no push option, it MUST set the push flag to the Push column of section 7 for that type. | A read receipt that pushes wakes every device for nothing; a text that does not push is a message nobody sees until they open the app. |

## 5. Nested content

A reply carries the content it replies with as a complete `EncodedContent` inside its own `content`, with the nested identifier repeated in human-readable form in the `contentType` parameter. The nested content is decoded with the same rule as a top-level one, so a reply with a custom type inside it is a reply the recipient can still place in the conversation: the reference resolves and the outer fallback shows. The `editMessage` schema in section 7 nests the same way.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| CTYPE-011 | Nested content is complete | When the client encodes content that nests another, the nested bytes MUST be a complete `EncodedContent` with its own `type`, so that CTYPE-001 selects its codec. | |
| CTYPE-012 | Nested unknown content | When the client decodes a reply whose nested content it cannot decode under CTYPE-008, it MUST return the reply with its `reference` and the nested `EncodedContent` as custom content, and MUST NOT fail the reply. | A recipient that fails the whole reply loses the reference and shows the reply as unknown instead of as a reply to a known message. |

## 6. A legacy and a current version

A type changes its major version when its encoding changes. Old messages under the old major version are still in every conversation and every archive, so a client decodes both and publishes only the current one. The reaction type is the one such case: major version 1 is JSON, major version 2 is protobuf. Both are in the catalogue.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| CTYPE-013 | Both reaction versions decode | The client MUST decode `xmtp.org/reaction` under `version_major` 1 and 2 as the catalogue states, and MUST publish a new reaction under `version_major` 2 only. | |

## 7. The catalogue

The table below is the set of standard types. Each row is the exact identifier, what `content` holds, the parameters the schema reads, whether a fallback is set, the push value, and whether a `deleteMessage` may target a message of the type. `?PROC` is expected to require that a client applies a `deleteMessage` only to a stored message whose type is deletable and whose sender is the deleter or a super admin. The schema blocks that follow the table state each encoding. A JSON encoding is written as a WebIDL dictionary whose member names are the JSON keys; a member that is not `required` is omitted from the JSON when absent.

Two types under `coinbase.com`, `actions` and `intent`, ship in every client and are listed with the standard types. The sync message type is owned by `?SYNC`, which states its identifier and schema; it is not repeated here. `xmtp.org/editMessage:1.0` has a schema in this repository and no client encodes or decodes it; it is reserved and its schema is listed last.

| Type | Identifier | Content | Parameters | Fallback | Push | Deletable |
| --- | --- | --- | --- | --- | --- | --- |
| Text | `xmtp.org/text:1.0` | UTF-8 text | `encoding`, always `UTF-8`; absent reads as `UTF-8`; any other value fails decoding | no | true | yes |
| Markdown | `xmtp.org/markdown:1.0` | UTF-8 Markdown | `encoding`, as for text | no | true | yes |
| Reaction | `xmtp.org/reaction:2.0` | Protobuf `ReactionV2` | none | yes | false | no |
| Legacy reaction | `xmtp.org/reaction:1.0` | JSON `LegacyReaction` | none | yes | false | no |
| Reply | `xmtp.org/reply:1.0` | Protobuf `EncodedContent`: the nested content | `reference`: the replied-to message id in lowercase hexadecimal; `contentType`: the nested identifier in human-readable form; `referenceInboxId`: optional, the replied-to sender's inbox id | yes | true | yes |
| Read receipt | `xmtp.org/readReceipt:1.0` | Empty | none | no | false | no |
| Attachment | `xmtp.org/attachment:1.0` | The file bytes | `mimeType`; `filename`: optional | yes | true | yes |
| Remote attachment | `xmtp.org/remoteStaticAttachment:1.0` | The URL as UTF-8 text | `contentDigest`, `secret`, `salt`, `nonce`, `scheme`, `contentLength`: optional, `filename`: optional; see `RemoteAttachment` | yes | true | yes |
| Multiple remote attachments | `xmtp.org/multiRemoteStaticAttachment:1.0` | Protobuf `MultiRemoteAttachment` | none | yes | true | yes |
| Transaction reference | `xmtp.org/transactionReference:1.0` | JSON `TransactionReference` | none | yes | true | yes |
| Wallet send calls | `xmtp.org/walletSendCalls:1.0` | JSON `WalletSendCalls` | none | yes | true | yes |
| Actions | `coinbase.com/actions:1.0` | JSON `Actions` | none | yes | true | no |
| Intent | `coinbase.com/intent:1.0` | JSON `Intent` | none | yes | true | no |
| Group updated | `xmtp.org/group_updated:1.0` | Protobuf `GroupUpdated` | none | no | false | no |
| Legacy membership change | `xmtp.org/group_membership_change:1.0` | Protobuf `GroupMembershipChanges` | none | no | false | no |
| Leave request | `xmtp.org/leave_request:1.0` | Protobuf `LeaveRequest` | none | no | false | no |
| Delete message | `xmtp.org/deleteMessage:1.0` | Protobuf `DeleteMessage` | none | no | false | no |
| Edit message (reserved) | `xmtp.org/editMessage:1.0` | Protobuf `EditMessage` | none | | | |

Group updated and legacy membership change are never published: a client derives a group updated message from each commit it applies and stores it as a message of the group, and a legacy membership change is the same record from clients before the current transcript format. Both are in the catalogue because every client stores and decodes them as messages of the group. A client publishes a leave request under `leave_request`: the schema file's comment names `leaveRequest`, and that comment is wrong.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| CTYPE-014 | Catalogue encodings are binding | When the client encodes or decodes content of a catalogue type, it MUST use the identifier, the content encoding, and the parameters the table above and the schema blocks below state for that type, and MUST NOT publish content of that type under any other identifier. | Two clients that disagree on the bytes behind one identifier each see the other's messages as undecodable. |
| CTYPE-015 | Remote attachment payload | When the client encodes a remote attachment, the bytes at the URL MUST be an encoded `EncodedContent` of type `xmtp.org/attachment:1.0` encrypted with AES-GCM under a 256-bit key derived by HKDF-SHA256 from a 32-byte random `secret` and a 32-byte random `salt` with empty info, under a 12-byte random `nonce`, with `contentDigest` the lowercase hexadecimal SHA-256 of the ciphertext. When the client decrypts one, it MUST reject a payload whose SHA-256 differs from `contentDigest` before it decrypts. | The URL is unauthenticated storage. Without the digest check a substituted payload is decrypted and shown. |

### 7.1 Schemas

The nested content of a reply, and the remote attachment parameters, as internal shapes:

```webidl
dictionary RemoteAttachment {
  required DOMString url;             // the content bytes of the message
  required DOMString contentDigest;   // hex SHA-256 of the encrypted payload
  required sequence<octet> secret;    // 32 bytes; carried hex-encoded in the parameter
  required sequence<octet> salt;      // 32 bytes; carried hex-encoded
  required sequence<octet> nonce;     // 12 bytes; carried hex-encoded
  required DOMString scheme;          // "https://"
  unsigned long contentLength;        // encrypted payload length in bytes
  DOMString filename;
};
```

The protobuf encodings:

```proto
// Action enum to represent reaction states
enum ReactionAction {
  REACTION_ACTION_UNSPECIFIED = 0;
  REACTION_ACTION_ADDED = 1;
  REACTION_ACTION_REMOVED = 2;
}

// Schema enum to represent reaction content types
enum ReactionSchema {
  REACTION_SCHEMA_UNSPECIFIED = 0;
  REACTION_SCHEMA_UNICODE = 1;
  REACTION_SCHEMA_SHORTCODE = 2;
  REACTION_SCHEMA_CUSTOM = 3;
}

// Reaction message type
message ReactionV2 {
  // The message ID being reacted to
  string reference = 1;
  // The inbox ID of the user who sent the message being reacted to
  // Optional for group messages
  string reference_inbox_id = 2;
  // The action of the reaction (added or removed)
  ReactionAction action = 3;
  // The content of the reaction
  string content = 4;
  // The schema of the reaction content
  ReactionSchema schema = 5;
}
```

```proto
// MultiRemoteAttachment message type
message MultiRemoteAttachment {
  // Array of attachment information
  repeated RemoteAttachmentInfo attachments = 1;
}

message RemoteAttachmentInfo {
  // The SHA256 hash of the remote content
  string content_digest = 1;
  // A 32 byte array for decrypting the remote content payload
  bytes secret = 2;
  // A byte array for the nonce used to encrypt the remote content payload
  bytes nonce = 3;
  // A byte array for the salt used to encrypt the remote content payload
  bytes salt = 4;
  // The scheme of the URL. Must be "https://"
  string scheme = 5;
  // The URL of the remote content
  string url = 6;
  // The size of the encrypted content in bytes (max size of 4GB)
  optional uint32 content_length = 7;
  // The filename of the remote content
  optional string filename = 8;
}
```

Each `RemoteAttachmentInfo` is encrypted as CTYPE-015 states for a single remote attachment.

```proto
// A summary of the changes in a commit.
// Includes added/removed inboxes and changes to metadata
message GroupUpdated {
  // An inbox that was added or removed in this commit
  message Inbox {
    string inbox_id = 1;
  }

  // A summary of a change to the mutable metadata
  message MetadataFieldChange {
    // The field that was changed
    string field_name = 1;
    // The previous value
    optional string old_value = 2;
    // The updated value
    optional string new_value = 3;
  }

  string initiated_by_inbox_id = 1;
  // The inboxes added in the commit
  repeated Inbox added_inboxes = 2;
  // The inboxes removed in the commit
  repeated Inbox removed_inboxes = 3;
  // The metadata changes in the commit
  repeated MetadataFieldChange metadata_field_changes = 4;
  /// The inboxes that were removed from the group in response to pending-remove/self-remove requests
  repeated Inbox left_inboxes = 5;
  // The inboxes that were added to admin list in the commit
  repeated Inbox added_admin_inboxes = 6;
  // The inboxes that were removed from admin list in the commit
  repeated Inbox removed_admin_inboxes = 7;
  // The inboxes that were added to super admin list in the commit
  repeated Inbox added_super_admin_inboxes = 8;
  // The inboxes that were removed from super admin list in the commit
  repeated Inbox removed_super_admin_inboxes = 9;
}
```

```proto
// A group member and affected installation IDs
message MembershipChange {
  repeated bytes installation_ids = 1;
  string account_address = 2;
  string initiated_by_account_address = 3;
}

// The group membership change proto
message GroupMembershipChanges {
  // Members that have been added in the commit
  repeated MembershipChange members_added = 1;
  // Members that have been removed in the commit
  repeated MembershipChange members_removed = 2;
  // Installations that have been added in the commit, grouped by member
  repeated MembershipChange installations_added = 3;
  // Installations removed in the commit, grouped by member
  repeated MembershipChange installations_removed = 4;
}
```

```proto
// LeaveRequest message type
message LeaveRequest {
  // A serialized AuthenticatedNote containing the sender's signed, member-only verifiable statement
  optional bytes authenticated_note = 1;
}

// DeleteMessage message type
message DeleteMessage {
  // ID of the message to delete
  string message_id = 1;
}

// EditMessage message type
message EditMessage {
  // ID of the message to edit
  string message_id = 1;
  // The new content for the message
  xmtp.mls.message_contents.EncodedContent edited_content = 2;
}
```

`DeleteMessage.message_id` is the target message id in lowercase hexadecimal.

The JSON encodings:

```webidl
dictionary LegacyReaction {
  required DOMString action;        // "added" or "removed"
  required DOMString reference;     // the message id reacted to, hex
  DOMString referenceInboxId;
  required DOMString schema;        // "unicode", "shortcode", or "custom"
  required DOMString content;
};

dictionary TransactionMetadata {
  required DOMString transactionType;
  required DOMString currency;
  required double amount;
  required unsigned long decimals;
  required DOMString fromAddress;
  required DOMString toAddress;
};

dictionary TransactionReference {
  DOMString namespace;              // for example "eip155"
  required DOMString networkId;     // a JSON string; a JSON number is accepted on decode
  required DOMString reference;     // the transaction hash
  TransactionMetadata metadata;
};

dictionary WalletCallMetadata {
  required DOMString description;
  required DOMString transactionType;
  // any other member is kept as a string
};

dictionary WalletCall {
  DOMString to;                     // hex address
  DOMString data;                   // hex call data
  DOMString value;                  // hex value
  DOMString gas;                    // hex gas limit
  WalletCallMetadata metadata;
};

dictionary WalletSendCalls {
  required DOMString version;
  required DOMString chainId;       // hex chain id, for example "0x1"
  required DOMString from;          // hex address
  required sequence<WalletCall> calls;
  record<DOMString, DOMString> capabilities;
};

dictionary Action {
  required DOMString id;
  required DOMString label;
  DOMString imageUrl;
  DOMString style;                  // "primary", "secondary", or "danger"
  DOMString expiresAt;              // RFC 3339 with millisecond precision, UTC
};

dictionary Actions {
  required DOMString id;
  required DOMString description;
  required sequence<Action> actions; // 1 to 10 entries; ids unique
  DOMString expiresAt;              // RFC 3339 with millisecond precision, UTC
};

dictionary Intent {
  required DOMString id;
  required DOMString actionId;
  record<DOMString, any> metadata;  // at most 10240 bytes when encoded
};
```

## Known limitations

No client inflates `content` under `compression` before decoding: the Rust client and the TypeScript SDK ignore the field, and the Kotlin and Swift SDKs inflate only in the codec path an app calls. CTYPE-006 forbids setting it; content that sets it decodes on no client outside that path.

A client that receives content under an unknown major version of a catalogue type keeps it under CTYPE-008 and shows the fallback. It does not attempt the codec for the major version it has.

The catalogue lists the types every client ships. Not every SDK exposes a codec class for every one; an app on such an SDK sends and receives the type through the raw `EncodedContent`.

`editMessage` is reserved. A message of that type is kept under CTYPE-008 and has no effect on the message it names.

The push value binds a client that publishes through a codec. An app that builds an `EncodedContent` itself supplies the push flag itself, and a wrong choice costs only its own recipients a notification.
