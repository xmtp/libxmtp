# XMTP Architecture: Complete Reference Guide

> **Note:** This document covers the **d14n (decentralized) backend** architecture. The legacy v3 backend is not covered here. All network operations, endpoints, and flows described assume the d14n infrastructure.

## Table of Contents

1. [Core Identity Concepts](#core-identity-concepts)
2. [Group Concepts](#group-concepts)
3. [Network Message Types](#network-message-types)
4. [Local Message Types (Intents)](#local-message-types-intents)
5. [Stored Message Types](#stored-message-types)
6. [Content Types](#content-types)
7. [Cursor System](#cursor-system)
8. [Relationships Diagram](#relationships-diagram)
9. [Complete Message Flow Examples (D14n)](#complete-message-flow-examples-d14n)
10. [D14n Backend API Layer](#d14n-backend-api-layer)
11. [Key Source Files Reference](#key-source-files-reference)

---

## Crate Architecture

LibXMTP is organized as a Rust workspace:

- **xmtp_mls** - Core client, groups, and message handling
- **xmtp_id** - Identity and association management (covered in "Core Identity Concepts")
- **xmtp_db** - Encrypted SQLite storage (covered in "Stored Message Types")
- **xmtp_proto** - Protocol definitions and types (covered in "Network Message Types")
- **xmtp_api_d14n** - D14n backend API layer (covered in "D14n Backend API Layer")
- **bindings/** - Mobile (uniffi), WASM, and Node.js (napi) bindings

For development commands and testing, see [CLAUDE.md](../CLAUDE.md).

## Core Identity Concepts

### **Inbox ID**

The `inbox_id` is the **top-level identity** in XMTP - the unit of user identity across the system.

**Generation:**

```text
inbox_id = SHA256(wallet_address + nonce)
```

> **Source:** Generation logic in [`crates/xmtp_id/src/associations/member.rs:170`](crates/xmtp_id/src/associations/member.rs) (`Identifier::inbox_id()`)

**Properties:**

- Deterministic from wallet address + nonce
- One inbox can have multiple associated wallets (Ethereum, Passkeys)
- One inbox can have multiple installations (devices)
- **Unit of membership** in groups - you add/remove `inbox_id`s, not individual devices

---

### **Installation ID**

An `installation_id` represents a **single device/client instance**.

> **Source:** [`crates/xmtp_id/src/associations/ident/installation.rs:4`](crates/xmtp_id/src/associations/ident/installation.rs)

**Contents:**

```rust
// crates/xmtp_id/src/associations/ident/installation.rs
pub struct Installation(pub Vec<u8>);  // Ed25519 public key (32 bytes)

// crates/xmtp_id/src/associations/state.rs:30
pub struct Installation {
    pub id: Vec<u8>,
    pub client_timestamp_ns: Option<u64>,
}
```

**Properties:**

- Generated on first client initialization (random Ed25519 keypair)
- The public key IS the installation_id
- Used to sign MLS leaf nodes and key packages
- Messages are encrypted TO installations, not inbox_ids
- Multiple installations per inbox_id (phone, laptop, tablet, etc.)

---

### **Association State**

Tracks the current state of an inbox - what's associated with it.

> **Source:** [`crates/xmtp_id/src/associations/state.rs:58`](crates/xmtp_id/src/associations/state.rs)

**Contents:**

```rust
// crates/xmtp_id/src/associations/state.rs
pub struct AssociationState {
    pub(crate) inbox_id: String,
    pub(crate) members: HashMap<MemberIdentifier, Member>,  // All associated identifiers
    pub(crate) recovery_identifier: Identifier,             // Can recover the inbox
    pub(crate) seen_signatures: HashSet<Vec<u8>>,           // Replay protection
}
```

**Member Types:**

> **Source:** [`crates/xmtp_id/src/associations/member.rs:22`](crates/xmtp_id/src/associations/member.rs)

```rust
// crates/xmtp_id/src/associations/member.rs
pub enum MemberIdentifier {
    Installation(ident::Installation),   // Device public key
    Ethereum(ident::Ethereum),           // Wallet address (0x...)
    Passkey(ident::Passkey),             // Passkey credential
}
```

**Identifier (for wallets/passkeys only):**

> **Source:** [`crates/xmtp_id/src/associations/member.rs:33`](crates/xmtp_id/src/associations/member.rs)

```rust
// crates/xmtp_id/src/associations/member.rs
pub enum Identifier {
    Ethereum(ident::Ethereum),
    Passkey(ident::Passkey),
}
```

---

### **Sequence ID (for Identity)**

A version number for an inbox's association state.

**Usage:**

- Each `IdentityUpdate` increments the sequence_id
- Groups store `{inbox_id: sequence_id}` to know which installations should be members
- When sequence_id changes, installations may have been added or revoked

---

### **Identity Update**

A signed update to an inbox's association state.

> **Source:** [`crates/xmtp_id/src/associations/association_log.rs:358`](crates/xmtp_id/src/associations/association_log.rs)

**Contents:**

```rust
// crates/xmtp_id/src/associations/association_log.rs
pub struct IdentityUpdate {
    pub inbox_id: String,
    pub client_timestamp_ns: u64,
    pub actions: Vec<Action>,
}

// crates/xmtp_id/src/associations/association_log.rs:319
pub enum Action {
    CreateInbox(CreateInbox),              // Create new inbox
    AddAssociation(AddAssociation),        // Add wallet/installation
    RevokeAssociation(RevokeAssociation),  // Remove wallet/installation
    ChangeRecoveryIdentity(ChangeRecoveryIdentity),
}
```

**CreateInbox:**

> **Source:** [`crates/xmtp_id/src/associations/association_log.rs:67`](crates/xmtp_id/src/associations/association_log.rs)

```rust
// crates/xmtp_id/src/associations/association_log.rs
pub struct CreateInbox {
    pub nonce: u64,                              // Combined with address for inbox_id
    pub initial_identifier: Identifier,          // First wallet
    pub initial_identifier_signature: VerifiedSignature,
}
```

**AddAssociation:**

> **Source:** [`crates/xmtp_id/src/associations/association_log.rs:114`](crates/xmtp_id/src/associations/association_log.rs)

```rust
// crates/xmtp_id/src/associations/association_log.rs
pub struct AddAssociation {
    pub new_member_identifier: MemberIdentifier,     // What's being added
    pub new_member_signature: VerifiedSignature,     // Signed by new member
    pub existing_member_signature: VerifiedSignature, // Authorized by existing member
}
```

**RevokeAssociation:**

> **Source:** [`crates/xmtp_id/src/associations/association_log.rs:218`](crates/xmtp_id/src/associations/association_log.rs)

```rust
// crates/xmtp_id/src/associations/association_log.rs
pub struct RevokeAssociation {
    pub revoked_member: MemberIdentifier,
    pub recovery_identifier_signature: VerifiedSignature,
}
```

**ChangeRecoveryIdentity:**

> **Source:** [`crates/xmtp_id/src/associations/association_log.rs:278`](crates/xmtp_id/src/associations/association_log.rs)

```rust
// crates/xmtp_id/src/associations/association_log.rs
pub struct ChangeRecoveryIdentity {
    pub recovery_identifier_signature: VerifiedSignature,
    pub new_recovery_identifier: Identifier,
}
```

---

## Group Concepts

### **Group ID**

A unique identifier for an MLS group (conversation).

**Properties:**

- Randomly generated 16 bytes when group is created
- Used to query messages from the network
- Immutable for the lifetime of the group

---

### **MlsGroup (Runtime)**

The runtime representation of a group with all operations.

> **Source:** [`crates/xmtp_mls/src/groups/mod.rs:128`](crates/xmtp_mls/src/groups/mod.rs)

```rust
// crates/xmtp_mls/src/groups/mod.rs
pub struct MlsGroup<Context> {
    pub context: Context,
    pub group_id: Vec<u8>,
    pub dm_id: Option<String>,
    pub conversation_type: ConversationType,
    pub created_at_ns: i64,
}
```

### **StoredGroup (Database)**

The persisted representation of a group.

> **Source:** [`crates/xmtp_db/src/encrypted_store/group.rs:53`](crates/xmtp_db/src/encrypted_store/group.rs)

```rust
// crates/xmtp_db/src/encrypted_store/group.rs
pub struct StoredGroup {
    pub id: Vec<u8>,
    pub created_at_ns: i64,
    pub membership_state: GroupMembershipState,
    pub added_by_inbox_id: String,
    pub conversation_type: ConversationType,
    pub dm_id: Option<String>,
    // ... additional fields
}
```

---

### **Group Membership Extension**

An MLS GroupContextExtension mapping inbox_ids to their sequence_ids.

> **Source:** [`crates/xmtp_mls/src/groups/group_membership.rs:8`](crates/xmtp_mls/src/groups/group_membership.rs)

**Contents:**

```rust
// crates/xmtp_mls/src/groups/group_membership.rs
pub struct GroupMembership {
    pub members: HashMap<String, i64>,           // inbox_id -> sequence_id
    pub failed_installations: Vec<Vec<u8>>,      // Failed to add
}
```

**Example:**

```json
{
	"a88c44c7e73df4fb...": 24773779,
	"7cc8936c47867bbc...": 24773800
}
```

This means: "inbox `a88c44c7...` should have all installations that existed at sequence_id 24773779"

---

### **Conversation Type**

> **Source:** [`crates/xmtp_db/src/encrypted_store/group.rs:1255`](crates/xmtp_db/src/encrypted_store/group.rs)

```rust
// crates/xmtp_db/src/encrypted_store/group.rs
pub enum ConversationType {
    Group = 1,   // Multi-party conversation (any number of members)
    Dm = 2,      // Direct message (exactly 2 inbox_ids)
    Sync = 3,    // Device sync (internal use)
}
```

---

### **DM ID**

For DMs, a deterministic identifier combining both participants.

> **Source:** [`crates/xmtp_mls_common/src/group_metadata.rs:114`](crates/xmtp_mls_common/src/group_metadata.rs)

```rust
// crates/xmtp_mls_common/src/group_metadata.rs
pub struct DmMembers<Id: AsRef<str>> {
    pub member_one_inbox_id: Id,
    pub member_two_inbox_id: Id,
}
```

**Format:**

```text
dm_id = "{member_one_inbox_id}:{member_two_inbox_id}"
```

Used to find existing DMs and prevent duplicates.

---

### **Group Context Extensions**

XMTP uses these MLS extensions in every group:

> **Extension IDs defined in:** [`crates/xmtp_configuration/src/common/metadata.rs`](crates/xmtp_configuration/src/common/metadata.rs)

```rust
// crates/xmtp_configuration/src/common/metadata.rs
pub const MUTABLE_METADATA_EXTENSION_ID: u16 = 0xff00;
pub const GROUP_MEMBERSHIP_EXTENSION_ID: u16 = 0xff01;
pub const GROUP_PERMISSIONS_EXTENSION_ID: u16 = 0xff02;
```

| Extension                 | Purpose                                | Source File                                            | Governed By                                                         |
| ------------------------- | -------------------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------- |
| `GroupMembership`         | Maps inbox_ids → sequence_ids          | `crates/xmtp_mls/src/groups/group_membership.rs`       | `add_member_policy`, `remove_member_policy`                         |
| `GroupMutableMetadata`    | Name, description, admin lists         | `crates/xmtp_mls_common/src/group_mutable_metadata.rs` | `update_metadata_policy`, `add_admin_policy`, `remove_admin_policy` |
| `GroupMutablePermissions` | Policy rules for the group             | `crates/xmtp_mls/src/groups/group_permissions.rs`      | `update_permissions_policy`                                         |
| `ImmutableMetadata`       | Conversation type, creator, DM members | `crates/xmtp_mls_common/src/group_metadata.rs`         | Immutable                                                           |

**GroupMetadata (Immutable):**

> **Source:** [`crates/xmtp_mls_common/src/group_metadata.rs:36`](crates/xmtp_mls_common/src/group_metadata.rs)

```rust
// crates/xmtp_mls_common/src/group_metadata.rs
pub struct GroupMetadata {
    pub conversation_type: ConversationType,
    pub creator_inbox_id: String,
    pub dm_members: Option<DmMembers<String>>,
    pub oneshot_message: Option<OneshotMessage>,
}
```

**GroupMutableMetadata:**

> **Source:** [`crates/xmtp_mls_common/src/group_mutable_metadata.rs:103`](crates/xmtp_mls_common/src/group_mutable_metadata.rs)

```rust
// crates/xmtp_mls_common/src/group_mutable_metadata.rs
pub struct GroupMutableMetadata {
    pub attributes: HashMap<String, String>,  // name, description, etc.
    pub admin_list: Vec<String>,              // inbox_ids with admin role
    pub super_admin_list: Vec<String>,        // inbox_ids with super_admin role
}
```

**PolicySet:**

> **Source:** [`crates/xmtp_mls/src/groups/group_permissions.rs:886`](crates/xmtp_mls/src/groups/group_permissions.rs)

```rust
// crates/xmtp_mls/src/groups/group_permissions.rs
pub struct PolicySet {
    pub add_member_policy: MembershipPolicy,
    pub remove_member_policy: MembershipPolicy,
    pub add_admin_policy: AdminPolicy,
    pub remove_admin_policy: AdminPolicy,
    pub update_group_name_policy: MetadataPolicy,
    pub update_group_description_policy: MetadataPolicy,
    pub update_group_image_policy: MetadataPolicy,
    pub update_group_pinned_frame_policy: MetadataPolicy,
    pub update_permissions_policy: PermissionsPolicy,
}
```

---

## Network Message Types

### **1. Key Package**

**Purpose:** Public credential for receiving Welcome messages

> **Source:** [`crates/xmtp_mls/src/verified_key_package_v2.rs:51`](crates/xmtp_mls/src/verified_key_package_v2.rs)

**Contents:**

```rust
// crates/xmtp_mls/src/verified_key_package_v2.rs
pub struct VerifiedKeyPackageV2 {
    pub inner: KeyPackage,              // MLS KeyPackage (from openmls)
    pub credential: MlsCredential,      // Contains inbox_id
    pub installation_public_key: Vec<u8>, // Ed25519 public key
}
```

**XMTP KeyPackage Builder:**

> **Source:** [`crates/xmtp_mls/src/identity.rs:701`](crates/xmtp_mls/src/identity.rs)

```rust
// crates/xmtp_mls/src/identity.rs
pub struct XmtpKeyPackage {
    inbox_id: String,
    credential: OpenMlsCredential,
    installation_keys: XmtpInstallationCredential,
}
```

**MLS KeyPackage contains:**

- `hpke_init_key`: Public key for encrypting Welcomes (X25519)
- `leaf_node`: Contains signature key and credential
- `lifetime`: Validity period (not_before, not_after)
- `extensions`: Including wrapper encryption key for post-quantum

**Validation:**

1. Standard MLS signature checks
2. Verify `installation_public_key` is associated with `inbox_id` in credential
3. Check lifetime validity

**Rotation:** After receiving a Welcome (key was used)

---

### **2. Welcome Message**

**Purpose:** Invite an installation to join a group

> **Source:** [`crates/xmtp_proto/src/types/welcome_message.rs:15`](crates/xmtp_proto/src/types/welcome_message.rs)

**Contents:**

```rust
// crates/xmtp_proto/src/types/welcome_message.rs
pub struct WelcomeMessage {
    pub cursor: Cursor,                      // Position in welcome stream
    pub created_ns: DateTime<Utc>,           // Server timestamp
    pub variant: WelcomeMessageType,
}

// crates/xmtp_proto/src/types/welcome_message.rs:60
pub enum WelcomeMessageType {
    V1(WelcomeMessageV1),                // Standard welcome
    WelcomePointer(WelcomePointer),      // Post-quantum pointer
    DecryptedWelcomePointer(DecryptedWelcomePointer),
}
```

**WelcomeMessageV1:**

> **Source:** [`crates/xmtp_proto/src/types/welcome_message.rs:86`](crates/xmtp_proto/src/types/welcome_message.rs)

```rust
// crates/xmtp_proto/src/types/welcome_message.rs
pub struct WelcomeMessageV1 {
    pub installation_key: InstallationId,         // Recipient's installation
    pub hpke_public_key: Vec<u8>,                 // Key used for HPKE encryption
    pub wrapper_algorithm: WelcomeWrapperAlgorithm, // Curve25519 or post-quantum
    pub data: Vec<u8>,                            // Encrypted MLS Welcome
    pub welcome_metadata: Vec<u8>,                // Encrypted metadata
}
```

**WelcomePointer (Post-Quantum):**

> **Source:** [`crates/xmtp_proto/src/types/welcome_message.rs:107`](crates/xmtp_proto/src/types/welcome_message.rs)

```rust
// crates/xmtp_proto/src/types/welcome_message.rs
pub struct WelcomePointer {
    pub installation_key: InstallationId,
    pub hpke_public_key: Vec<u8>,
    pub wrapper_algorithm: WelcomePointerWrapperAlgorithm,
    pub welcome_pointer: Vec<u8>,
}
```

**DecryptedWelcomePointer:**

> **Source:** [`crates/xmtp_proto/src/types/welcome_message.rs:126`](crates/xmtp_proto/src/types/welcome_message.rs)

```rust
// crates/xmtp_proto/src/types/welcome_message.rs
pub struct DecryptedWelcomePointer {
    pub destination: InstallationId,
    pub aead_type: WelcomePointeeEncryptionAeadType,
    pub encryption_key: Vec<u8>,
    pub data_nonce: Vec<u8>,
    pub welcome_metadata_nonce: Vec<u8>,
}
```

**Encryption layers:**

1. **Outer layer:** HPKE encryption using recipient's key package `hpke_init_key`
2. **Inner layer:** Standard MLS Welcome (contains ratchet tree, group secrets)

**Contains (after decryption):**

- Complete MLS ratchet tree
- Group context with all extensions
- Encryption secrets for the current epoch
- Information about who added you (`added_by_inbox_id`)

**Processing:**

> **Source:** [`crates/xmtp_mls/src/groups/welcomes/xmtp_welcome.rs`](crates/xmtp_mls/src/groups/welcomes/xmtp_welcome.rs)

---

### **3. Group Message**

**Purpose:** Container for MLS protocol messages (both commits and application messages)

> **Source:** [`crates/xmtp_proto/src/types/group_message.rs:10`](crates/xmtp_proto/src/types/group_message.rs)

**Contents:**

```rust
// crates/xmtp_proto/src/types/group_message.rs
pub struct GroupMessage {
    pub cursor: Cursor,                    // Position in message stream
    pub created_ns: DateTime<Utc>,         // Server timestamp
    pub group_id: GroupId,                 // Which group
    pub message: ProtocolMessage,          // MLS PrivateMessage or PublicMessage
    pub sender_hmac: Vec<u8>,              // For sender authentication
    pub should_push: bool,                 // Trigger push notification?
    pub payload_hash: Vec<u8>,             // Hash for deduplication
    pub depends_on: GlobalCursor,          // Causal dependencies
}
```

**The `message` field can be:**

#### **3a. Commit (MLS Handshake)**

When `message.content_type() == ContentType::Commit`

**Purpose:** Update group state (membership, metadata, keys)

**Contains:**

```rust
Commit {
    proposals: Vec<ProposalOrRef>,  // What changes to make
    path: Option<UpdatePath>,       // New encryption keys (forward secrecy)
}
```

**Proposal Types:**

| Proposal                 | Purpose                                             |
| ------------------------ | --------------------------------------------------- |
| `Add`                    | Add an installation to the group                    |
| `Remove`                 | Remove an installation from the group               |
| `Update`                 | Update own leaf node (key rotation)                 |
| `GroupContextExtensions` | Change membership mapping, metadata, or permissions |

**Validation (XMTP-specific):**

> **Source:** [`crates/xmtp_mls/src/groups/validated_commit.rs:294`](crates/xmtp_mls/src/groups/validated_commit.rs)

```rust
// crates/xmtp_mls/src/groups/validated_commit.rs
pub struct ValidatedCommit {
    pub actor: CommitParticipant,
    pub added_inboxes: Vec<Inbox>,
    pub removed_inboxes: Vec<Inbox>,
    pub readded_installations: HashSet<Vec<u8>>,
    pub metadata_validation_info: MutableMetadataValidationInfo,
    pub installations_changed: bool,
    pub permissions_changed: bool,
    pub dm_members: Option<DmMembers<String>>,
}
```

1. All OpenMLS validations
2. Check permissions policy allows this action
3. Verify actual changes match expected diff from GroupMembership

#### **3b. Application Message**

When `message.content_type() == ContentType::Application`

**Purpose:** User content (chat messages)

**Encryption:**

- MLS secret tree + AEAD (ChaCha20-Poly1305)
- Different keys for each sender per epoch

**After decryption, contains:**

- Serialized `EncodedContent` with content type and payload

**EncodedContent:**

> **Source:** [`crates/xmtp_proto/src/gen/xmtp.mls.message_contents.rs:307`](crates/xmtp_proto/src/gen/xmtp.mls.message_contents.rs)

```rust
// crates/xmtp_proto/src/gen/xmtp.mls.message_contents.rs
pub struct EncodedContent {
    pub r#type: Option<ContentTypeId>,
    pub parameters: HashMap<String, String>,
    pub fallback: Option<String>,
    pub compression: Option<i32>,
    pub content: Vec<u8>,
}
```

---

## Local Message Types (Intents)

**Intents** are local commitments stored in SQLite before publishing to the network.

> **Source:** [`crates/xmtp_mls/src/intents.rs`](crates/xmtp_mls/src/intents.rs) (concept), [`crates/xmtp_db/src/encrypted_store/group_intent.rs`](crates/xmtp_db/src/encrypted_store/group_intent.rs) (storage)

### **Intent Structure**

> **Source:** [`crates/xmtp_db/src/encrypted_store/group_intent.rs:77`](crates/xmtp_db/src/encrypted_store/group_intent.rs)

```rust
// crates/xmtp_db/src/encrypted_store/group_intent.rs
pub struct StoredGroupIntent {
    pub id: i32,                        // Auto-generated ID
    pub kind: IntentKind,               // What type of action
    pub group_id: Vec<u8>,              // Which group
    pub data: Vec<u8>,                  // Serialized intent data
    pub state: IntentState,             // Current processing state
    pub payload_hash: Option<Vec<u8>>,  // Hash after publishing
    pub post_commit_data: Option<Vec<u8>>, // Data for post-commit actions
    pub publish_attempts: i32,          // Retry count
    pub cursor: Option<Cursor>,         // Network position when committed
}
```

### **Intent Kinds**

> **Source:** [`crates/xmtp_db/src/encrypted_store/group_intent.rs:38`](crates/xmtp_db/src/encrypted_store/group_intent.rs)

```rust
// crates/xmtp_db/src/encrypted_store/group_intent.rs
pub enum IntentKind {
    SendMessage = 1,           // Send application message
    KeyUpdate = 2,             // Rotate path encryption keys
    MetadataUpdate = 3,        // Update group name, description, etc.
    UpdateGroupMembership = 4, // Add/remove inbox_ids
    UpdateAdminList = 5,       // Change admin/super_admin lists
    UpdatePermission = 6,      // Change group permission policies
    ReaddInstallations = 7,    // Re-add installations after revocation
}
```

### **Intent States**

> **Source:** [`crates/xmtp_db/src/encrypted_store/group_intent.rs:66`](crates/xmtp_db/src/encrypted_store/group_intent.rs)

```rust
// crates/xmtp_db/src/encrypted_store/group_intent.rs
pub enum IntentState {
    ToPublish = 1,   // Ready to send to network
    Published = 2,   // Sent, waiting for confirmation
    Committed = 3,   // Confirmed on network (in our commit)
    Error = 4,       // Failed permanently
    Processed = 5,   // Fully processed (could be someone else's commit)
}
```

### **Intent Data Structures**

**SendMessageIntentData:**

> **Source:** [`crates/xmtp_mls/src/groups/intents.rs:88`](crates/xmtp_mls/src/groups/intents.rs)

```rust
// crates/xmtp_mls/src/groups/intents.rs
pub struct SendMessageIntentData {
    pub message: Vec<u8>,  // Serialized EncodedContent
}
```

**UpdateMetadataIntentData:**

> **Source:** [`crates/xmtp_mls/src/groups/intents.rs:183`](crates/xmtp_mls/src/groups/intents.rs)

```rust
// crates/xmtp_mls/src/groups/intents.rs
pub struct UpdateMetadataIntentData {
    pub field_name: String,        // "group_name", "description", etc.
    pub field_value: String,
}
```

**UpdateAdminListIntentData:**

> **Source:** [`crates/xmtp_mls/src/groups/intents.rs:448`](crates/xmtp_mls/src/groups/intents.rs)

```rust
// crates/xmtp_mls/src/groups/intents.rs
pub struct UpdateAdminListIntentData {
    pub admin_list_update_type: AdminListActionType,
    pub inbox_id: String,
}
```

**AdminListActionType:**

> **Source:** [`crates/xmtp_mls/src/groups/intents.rs:426`](crates/xmtp_mls/src/groups/intents.rs)

```rust
// crates/xmtp_mls/src/groups/intents.rs
pub enum AdminListActionType {
    Add = 1,
    Remove = 2,
    AddSuper = 3,
    RemoveSuper = 4,
}
```

**Intent Queue Operations:**

> **Source:** [`crates/xmtp_mls/src/groups/intents/queue.rs`](crates/xmtp_mls/src/groups/intents/queue.rs)

---

## Stored Message Types

### **StoredGroupMessage**

Messages stored in local SQLite after processing.

> **Source:** [`crates/xmtp_db/src/encrypted_store/group_message.rs:46`](crates/xmtp_db/src/encrypted_store/group_message.rs)

```rust
// crates/xmtp_db/src/encrypted_store/group_message.rs
pub struct StoredGroupMessage {
    pub id: Vec<u8>,                     // Unique message ID (hash-based)
    pub group_id: Vec<u8>,               // Which group
    pub decrypted_message_bytes: Vec<u8>, // Decrypted content
    pub sent_at_ns: i64,                 // When sent
    pub kind: GroupMessageKind,          // Application or MembershipChange
    pub sender_installation_id: Vec<u8>, // Which device sent it
    pub sender_inbox_id: String,         // Which user sent it
    pub delivery_status: DeliveryStatus, // Published/Unpublished/Failed
    pub content_type: ContentType,       // Text, Reaction, etc.
    pub version_major: i32,              // Content type version
    pub version_minor: i32,
    pub authority_id: String,            // Content type authority (e.g., "xmtp.org")
    pub reference_id: Option<Vec<u8>>,   // For replies/reactions
    pub originator_id: i64,              // Network originator
    pub sequence_id: i64,                // Network sequence
}
```

**Message ID Generation:**

> **Source:** [`crates/xmtp_mls/src/utils/mod.rs:34`](crates/xmtp_mls/src/utils/mod.rs)

```rust
// crates/xmtp_mls/src/utils/mod.rs
pub fn calculate_message_id(
    group_id: &[u8],
    decrypted_message_bytes: &[u8],
    sent_at_ns: i64,
) -> Vec<u8>
```

### **GroupMessageKind**

> **Source:** [`crates/xmtp_db/src/encrypted_store/group_message.rs:163`](crates/xmtp_db/src/encrypted_store/group_message.rs)

```rust
// crates/xmtp_db/src/encrypted_store/group_message.rs
pub enum GroupMessageKind {
    Application = 1,      // User-sent message (deletable)
    MembershipChange = 2, // Transcript message (not deletable)
}
```

### **DeliveryStatus**

> **Source:** [`crates/xmtp_db/src/encrypted_store/group_message.rs:377`](crates/xmtp_db/src/encrypted_store/group_message.rs)

```rust
// crates/xmtp_db/src/encrypted_store/group_message.rs
pub enum DeliveryStatus {
    Unpublished = 1,  // Stored locally, not yet sent
    Published = 2,    // Sent to network
    Failed = 3,       // Failed to send
}
```

### **DecodedMessage (Runtime)**

> **Source:** [`crates/xmtp_mls/src/messages/decoded_message.rs:108`](crates/xmtp_mls/src/messages/decoded_message.rs)

```rust
// crates/xmtp_mls/src/messages/decoded_message.rs
pub struct DecodedMessage {
    pub metadata: DecodedMessageMetadata,
    pub body: MessageBody,
}

// crates/xmtp_mls/src/messages/decoded_message.rs:84
pub struct DecodedMessageMetadata {
    pub id: Vec<u8>,
    pub group_id: Vec<u8>,
    pub sent_at_ns: i64,
    pub kind: GroupMessageKind,
    pub sender_installation_id: Vec<u8>,
    pub sender_inbox_id: String,
    pub delivery_status: DeliveryStatus,
    pub content_type: ContentTypeId,
    pub inserted_at_ns: i64,
    pub expires_at_ns: Option<i64>,
}
```

---

## Content Types

### **ContentType Enum (Database)**

> **Source:** [`crates/xmtp_db/src/encrypted_store/group_message.rs:212`](crates/xmtp_db/src/encrypted_store/group_message.rs)

```rust
// crates/xmtp_db/src/encrypted_store/group_message.rs
pub enum ContentType {
    Unknown = 0,
    Text = 1,
    GroupMembershipChange = 2,   // Legacy
    GroupUpdated = 3,            // Membership change transcript
    Reaction = 4,
    ReadReceipt = 5,
    Reply = 6,
    Attachment = 7,
    RemoteAttachment = 8,
    TransactionReference = 9,
    WalletSendCalls = 10,
    LeaveRequest = 11,
    Markdown = 12,
    Actions = 13,
    Intent = 14,
    MultiRemoteAttachment = 15,
    DeleteMessage = 16,
}
```

### **MessageBody (Decoded)**

> **Source:** [`crates/xmtp_mls/src/messages/decoded_message.rs:61`](crates/xmtp_mls/src/messages/decoded_message.rs)

```rust
// crates/xmtp_mls/src/messages/decoded_message.rs
pub enum MessageBody {
    Text(Text),
    Markdown(Markdown),
    Reply(Reply),
    Reaction(ReactionV2),
    Attachment(Attachment),
    RemoteAttachment(RemoteAttachment),
    MultiRemoteAttachment(MultiRemoteAttachment),
    TransactionReference(TransactionReference),
    GroupUpdated(GroupUpdated),         // Transcript of membership changes
    ReadReceipt(ReadReceipt),
    WalletSendCalls(WalletSendCalls),
    Intent(Option<Intent>),
    Actions(Option<Actions>),
    LeaveRequest(LeaveRequest),
    DeletedMessage { deleted_by: DeletedBy },
    Custom(EncodedContent),
}
```

### **Content Type Details**

**Text:**

> **Source:** [`crates/xmtp_mls/src/messages/decoded_message.rs:42`](crates/xmtp_mls/src/messages/decoded_message.rs)

```rust
// crates/xmtp_mls/src/messages/decoded_message.rs
pub struct Text {
    pub content: String,
}
```

**Reply:**

> **Source:** [`crates/xmtp_mls/src/messages/decoded_message.rs:32`](crates/xmtp_mls/src/messages/decoded_message.rs), [`crates/xmtp_content_types/src/reply.rs:103`](crates/xmtp_content_types/src/reply.rs)

```rust
// crates/xmtp_mls/src/messages/decoded_message.rs
pub struct Reply {
    pub reference_id: Vec<u8>,   // ID of message being replied to
    pub content: EncodedContent, // The reply content
}
```

**Reaction:**

> **Source:** [`crates/xmtp_proto/src/gen/xmtp.mls.message_contents.content_types.rs:92`](crates/xmtp_proto/src/gen/xmtp.mls.message_contents.content_types.rs)

```rust
// crates/xmtp_proto/src/gen/xmtp.mls.message_contents.content_types.rs
pub struct ReactionV2 {
    pub reference_id: Vec<u8>,   // ID of message being reacted to
    pub action: i32,             // Add or Remove (ReactionAction enum)
    pub content: String,         // Emoji or reaction content
    pub schema: i32,             // ReactionSchema enum
}
```

**GroupUpdated (Transcript):**

> **Source:** [`crates/xmtp_proto/src/gen/xmtp.mls.message_contents.rs:1087`](crates/xmtp_proto/src/gen/xmtp.mls.message_contents.rs)

```rust
// crates/xmtp_proto/src/gen/xmtp.mls.message_contents.rs
pub struct GroupUpdated {
    pub initiated_by_inbox_id: String,
    pub added_inboxes: Vec<Inbox>,
    pub removed_inboxes: Vec<Inbox>,
    pub metadata_field_changes: Vec<MetadataFieldChange>,
}
```

**Content Type Codecs:**

> **Source:** [`crates/xmtp_content_types/src/`](crates/xmtp_content_types/src/)

| Content Type     | Codec File                                           |
| ---------------- | ---------------------------------------------------- |
| Text             | `crates/xmtp_content_types/src/text.rs`              |
| Reply            | `crates/xmtp_content_types/src/reply.rs`             |
| Reaction         | `crates/xmtp_content_types/src/reaction.rs`          |
| GroupUpdated     | `crates/xmtp_content_types/src/group_updated.rs`     |
| RemoteAttachment | `crates/xmtp_content_types/src/remote_attachment.rs` |
| ReadReceipt      | `crates/xmtp_content_types/src/read_receipt.rs`      |

---

## Cursor System

### **Cursor**

A position marker in a message stream.

> **Source:** [`crates/xmtp_proto/src/types/cursor.rs:20`](crates/xmtp_proto/src/types/cursor.rs)

```rust
// crates/xmtp_proto/src/types/cursor.rs
pub struct Cursor {
    pub sequence_id: u64,      // Position in the stream
    pub originator_id: u32,    // Which node/service originated this
}
```

**Display format:** `[sid(123456):oid(0)]`

### **Originator IDs**

Different message types come from different "originators":

> **Source:** [`crates/xmtp_configuration/src/common/d14n.rs:5`](crates/xmtp_configuration/src/common/d14n.rs)

```rust
// crates/xmtp_configuration/src/common/d14n.rs
pub struct Originators;

impl Originators {
    pub const MLS_COMMITS: u32 = 0;              // Commit messages
    pub const APPLICATION_MESSAGES: u32 = 10;   // Application messages
    pub const WELCOME_MESSAGES: u32 = 11;       // Welcome messages
    pub const INSTALLATIONS: u32 = 13;          // Key package updates
}
```

### **GlobalCursor (Vector Clock)**

Tracks position across ALL originators.

> **Source:** [`crates/xmtp_proto/src/types/global_cursor.rs:22`](crates/xmtp_proto/src/types/global_cursor.rs)

```rust
// crates/xmtp_proto/src/types/global_cursor.rs
pub struct GlobalCursor {
    inner: BTreeMap<OriginatorId, SequenceId>
}
```

**Example:**

```json
{
	"0": 113635832, // Seen commits up to 113635832
	"10": 50000, // Seen app messages up to 50000
	"11": 25000 // Seen welcomes up to 25000
}
```

### **EntityKind (for refresh state)**

> **Source:** [`crates/xmtp_db/src/encrypted_store/refresh_state.rs:29`](crates/xmtp_db/src/encrypted_store/refresh_state.rs)

```rust
// crates/xmtp_db/src/encrypted_store/refresh_state.rs
pub enum EntityKind {
    Welcome = 1,              // Welcome messages
    ApplicationMessage = 2,   // User content (originator 10)
    CommitLogUpload = 3,      // Local commit log position
    CommitLogDownload = 4,    // Remote commit log position
    CommitLogForkCheckLocal = 5,
    CommitLogForkCheckRemote = 6,
    CommitMessage = 7,        // MLS commits (originator 0)
}
```

---

## Relationships Diagram

```text
┌───────────────────────────────────────────────────────────────────────────┐
│                              IDENTITY LAYER                               │
│                         (crates/xmtp_id/)                                 │
├───────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │                         INBOX_ID                                   │   │
│  │     (SHA256(wallet_address + nonce))                               │   │
│  │     Source: crates/xmtp_id/src/associations/member.rs:170          │   │
│  │                                                                    │   │
│  │   Association State @ sequence_id = N                              │   │
│  │   Source: crates/xmtp_id/src/associations/state.rs:58              │   │
│  │   ┌────────────────────────────────────────────────────────────┐   │   │
│  │   │  Members:                                                  │   │   │
│  │   │    - Ethereum("0x1234...")     ← wallet                    │   │   │
│  │   │    - Passkey(bytes)            ← passkey                   │   │   │
│  │   │    - Installation(pubkey1)     ← device 1                  │   │   │
│  │   │    - Installation(pubkey2)     ← device 2                  │   │   │
│  │   │    - Installation(pubkey3)     ← device 3                  │   │   │
│  │   └────────────────────────────────────────────────────────────┘   │   │
│  └────────────────────────────────────────────────────────────────────┘   │
│                                                                           │
│  Identity Updates (on-chain/network):                                     │
│  Source: crates/xmtp_id/src/associations/association_log.rs:358           │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐                      │
│  │CreateInbox│→│AddAssoc  │→│AddAssoc  │→│Revoke    │→ ...                │
│  │seq_id=1  │ │seq_id=2  │ │seq_id=3  │ │seq_id=4  │                      │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘                      │
└───────────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ inbox_ids are members of
                                    ▼
┌───────────────────────────────────────────────────────────────────────────────────────┐
│                              GROUP LAYER                                              │
│                         (crates/xmtp_mls/)                                            │
├───────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                       │
│  ┌────────────────────────────────────────────────────────────────────────────────┐   │
│  │           GROUP (group_id: 0x08513d45...)                                      │   │
│  │           Source: crates/xmtp_mls/src/groups/mod.rs:128                        │   │
│  │                                                                                │   │
│  │  GroupMembership Extension:                                                    │   │
│  │  Source: crates/xmtp_mls/src/groups/group_membership.rs:8                      │   │
│  │  ┌────────────────────────────────────────────────────────────────┐            │   │
│  │  │ "inbox_a88c44...": 24773779  ← use installations @ seq 24773779│            │   │
│  │  │ "inbox_7cc893...": 24773800  ← use installations @ seq 24773800│            │   │
│  │  └────────────────────────────────────────────────────────────────┘            │   │
│  │                                                                                │   │
│  │  Actual MLS Members (Installations):                                           │   │
│  │  ┌────────────────────────────────────────────────────────────┐                │   │
│  │  │ [pubkey1, pubkey2, pubkey3]  ← from inbox_a88c44           │                │   │
│  │  │ [pubkey4, pubkey5]           ← from inbox_7cc893           │                │   │
│  │  └────────────────────────────────────────────────────────────┘                │   │
│  │                                                                                │   │
│  │  Other Extensions:                                                             │   │
│  │  - GroupMutableMetadata (crates/xmtp_mls_common/src/group_mutable_metadata.rs) │   │
│  │  - GroupMutablePermissions (crates/xmtp_mls/src/groups/group_permissions.rs)   │   │
│  │  - ImmutableMetadata (crates/xmtp_mls_common/src/group_metadata.rs)            │   │
│  └────────────────────────────────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ messages sent to group
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                            NETWORK LAYER                               │
│                         (crates/xmtp_proto/)                           │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│  Per-Installation Stream (Welcome Messages):                           │
│  Source: crates/xmtp_proto/src/types/welcome_message.rs                │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐                                │
│  │ Welcome  │ │ Welcome  │ │ Welcome  │                                │
│  │cursor:100│ │cursor:101│ │cursor:102│                                │
│  │ group_a  │ │ group_b  │ │ group_c  │                                │
│  └──────────┘ └──────────┘ └──────────┘                                │
│                                                                        │
│  Per-Group Stream (Group Messages):                                    │
│  Source: crates/xmtp_proto/src/types/group_message.rs                  │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐      │
│  │  Commit  │ │   App    │ │   App    │ │  Commit  │ │   App    │      │
│  │  (Add)   │ │  (Text)  │ │(Reaction)│ │ (Remove) │ │  (Text)  │      │
│  │cursor:1  │ │cursor:2  │ │cursor:3  │ │cursor:4  │ │cursor:5  │      │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘      │
│                                                                        │
│  Key Package Registry:                                                 │
│  Source: crates/xmtp_mls/src/verified_key_package_v2.rs                │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │ installation_pubkey → KeyPackage (hpke_key, credential, lifetime)│  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ stored locally after processing
                                    ▼
┌────────────────────────────────────────────────────────────────────────────┐
│                            LOCAL STORAGE (SQLite)                          │
│                         (crates/xmtp_db/)                                  │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│  groups table:                                                             │
│  Source: crates/xmtp_db/src/encrypted_store/group.rs:53                    │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ id | created_at | membership_state | conversation_type | dm_id | ...│   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                            │
│  group_messages table:                                                     │
│  Source: crates/xmtp_db/src/encrypted_store/group_message.rs:46            │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ id | group_id | decrypted_bytes | kind | sender_inbox_id | ...      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                            │
│  group_intents table (pending actions):                                    │
│  Source: crates/xmtp_db/src/encrypted_store/group_intent.rs:77             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ id | group_id | kind | state | data | payload_hash | ...            │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                            │
│  refresh_state table (cursors):                                            │
│  Source: crates/xmtp_db/src/encrypted_store/refresh_state.rs               │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ entity_id | entity_kind | originator_id | sequence_id               │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────────────┘
```

---

## Complete Message Flow Examples (D14n)

### **Creating a DM:**

```text
1. Client: find_or_create_dm_by_inbox_id(target_inbox)
   Source: crates/xmtp_mls/src/client.rs:595

2. Check local DB for existing DM with dm_id = "my_inbox:target_inbox"

3. If not found:
   a. Create MLS group with ConversationType::Dm
      Source: crates/xmtp_mls/src/groups/mod.rs:519
   b. Set dm_id = "my_inbox:target_inbox"
   c. Apply DM permissions (deny add/remove members)

4. Queue UpdateGroupMembership intent to add target_inbox

5. Fetch target's KeyPackages:
   D14n Endpoint: GetNewestEnvelope
   gRPC Path: /xmtp.xmtpv4.message_api.ReplicationApi/GetNewestEnvelope
   Topic: [0x03][target_installation_id] (KeyPackagesV1)
   Source: crates/xmtp_api_d14n/src/queries/d14n/mls.rs:66

6. Create MLS Commit with Add proposals for each installation
   Source: crates/xmtp_mls/src/groups/mls_sync.rs

7. Publish Commit to network:
   D14n Endpoint: PublishClientEnvelopes
   gRPC Path: /xmtp.xmtpv4.payer_api.PayerApi/PublishClientEnvelopes
   Topic: [0x00][group_id] (GroupMessagesV1)
   Source: crates/xmtp_api_d14n/src/queries/d14n/mls.rs:89

8. Generate and publish Welcome for each installation:
   D14n Endpoint: PublishClientEnvelopes
   Topic: [0x01][installation_id] (WelcomeMessagesV1)
   Source: crates/xmtp_api_d14n/src/queries/d14n/mls.rs:122

9. Target's installations receive Welcome:
   D14n Endpoint: QueryEnvelope (or SubscribeEnvelopes for streaming)
   gRPC Path: /xmtp.xmtpv4.message_api.ReplicationApi/QueryEnvelopes
   Topic: [0x01][installation_id] (WelcomeMessagesV1)
   Source: crates/xmtp_api_d14n/src/queries/d14n/mls.rs:193

   Processing:
   a. Decrypt outer HPKE layer
   b. Process MLS Welcome
      Source: crates/xmtp_mls/src/groups/welcomes/xmtp_welcome.rs
   c. Store group locally with membership_state = Pending
```

### **Sending a Message:**

```text
1. Client: group.send_message(content)
   Source: crates/xmtp_mls/src/groups/mod.rs:602

2. Queue SendMessage intent with serialized EncodedContent
   Source: crates/xmtp_mls/src/groups/intents/queue.rs

3. MlsGroup::publish_intents() processes the intent:
   Source: crates/xmtp_mls/src/groups/mls_sync.rs:2460
   a. Create MLS ApplicationMessage (PrivateMessage)
   b. TLS serialize the message
   c. Compute SHA256 hash for dependency tracking

4. Find message dependencies from CursorStore:
   Source: crates/xmtp_api_d14n/src/queries/d14n/mls.rs:94
   - Looks up depends_on cursor for causal ordering

5. Publish to network:
   D14n Endpoint: PublishClientEnvelopes
   gRPC Path: /xmtp.xmtpv4.payer_api.PayerApi/PublishClientEnvelopes
   Request:
   {
     envelopes: [{
       aad: {
         target_topic: [0x00, group_id...],  // GroupMessagesV1
         depends_on: { cursor from step 4 }
       },
       payload: GroupMessageInput { data, sender_hmac, should_push }
     }]
   }
   Source: crates/xmtp_api_d14n/src/queries/d14n/mls.rs:111

6. Update intent state: ToPublish → Published
   Source: crates/xmtp_mls/src/groups/mls_sync.rs:2378

7. Recipients sync messages:
   D14n Endpoint: QueryEnvelope
   gRPC Path: /xmtp.xmtpv4.message_api.ReplicationApi/QueryEnvelopes
   Request:
   {
     topics: [[0x00, group_id...]],
     last_seen: { node_id_to_sequence_id: {...} },
     limit: 100
   }
   Source: crates/xmtp_api_d14n/src/queries/d14n/mls.rs:139

8. Process received messages:
   Source: crates/xmtp_mls/src/groups/mls_sync.rs:2048
   a. Order messages by depends_on (causal ordering)
   b. Decrypt using MLS secret tree
   c. Decode EncodedContent
   d. Store in group_messages table
   e. Update cursor in refresh_state table
```

### **Adding Members to Group:**

```text
1. Client: group.add_members_by_inbox_id([inbox_1, inbox_2])
   Source: crates/xmtp_mls/src/groups/mod.rs

2. Fetch latest identity updates for each inbox:
   D14n Endpoint: QueryEnvelopes
   Topic: [0x02][inbox_id_bytes] (IdentityUpdatesV1)
   Source: crates/xmtp_api_d14n/src/queries/d14n/identity.rs:61

3. Calculate expected installation diff
   Source: crates/xmtp_mls/src/groups/validated_commit.rs

4. Fetch KeyPackages for all new installations:
   D14n Endpoint: GetNewestEnvelope
   Topics: [[0x03][installation_id_1], [0x03][installation_id_2], ...]
   Source: crates/xmtp_api_d14n/src/queries/d14n/mls.rs:66

5. Queue UpdateGroupMembership intent

6. Create Commit with:
   - Add proposals for each new installation
   - GroupContextExtensions proposal updating GroupMembership

7. Publish Commit:
   D14n Endpoint: PublishClientEnvelopes
   Topic: [0x00][group_id] (GroupMessagesV1)

8. Generate and send Welcomes to new installations:
   D14n Endpoint: PublishClientEnvelopes
   Topics: [0x01][installation_id] for each new installation

9. Store transcript message (GroupUpdated) locally
```

### **Streaming Messages (Real-time):**

```text
1. Client: subscribe_group_messages([group_id_1, group_id_2])
   Source: crates/xmtp_api_d14n/src/queries/d14n/streams.rs:38

2. Create topics for each group:
   topics = [
     [0x00, group_id_1...],  // GroupMessagesV1
     [0x00, group_id_2...],
   ]

3. Get lowest common cursor from CursorStore:
   Source: crates/xmtp_api_d14n/src/protocol/traits/cursor_store.rs:54

4. Open streaming connection:
   D14n Endpoint: SubscribeEnvelopes
   gRPC Path: /xmtp.xmtpv4.message_api.ReplicationApi/SubscribeEnvelopes
   Request:
   {
     topics: [[0x00, group_id_1...], [0x00, group_id_2...]],
     last_seen: { node_id_to_sequence_id: {...} }
   }
   Source: crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_envelopes.rs

5. Stream processing pipeline:
   Source: crates/xmtp_api_d14n/src/queries/stream/

   XmtpStream (raw gRPC stream)
        │
        ▼
   FlattenedStream (unpack SubscribeEnvelopesResponse)
        │
        ▼
   OrderedStream (causal ordering, handle orphaned messages)
        │
        ▼
   TryExtractorStream<GroupMessageExtractor>
        │
        ▼
   Stream<Item = Result<GroupMessage, Error>>

6. For each message: decrypt, decode, store (same as sync flow)
```

---

## D14n Backend API Layer

The **d14n (decentralized) backend** is XMTP's production infrastructure. The `xmtp_api_d14n` crate provides the API layer for interacting with the decentralized network of xmtpd nodes.

> **Source:** [`crates/xmtp_api_d14n/src/lib.rs`](crates/xmtp_api_d14n/src/lib.rs)

---

### **Architecture Overview**

The d14n backend separates read and write operations:

| Operation   | Path                                        | Service        |
| ----------- | ------------------------------------------- | -------------- |
| **Writes**  | Client → Gateway → xmtpd nodes              | PayerApi       |
| **Reads**   | Client → Fastest xmtpd node                 | ReplicationApi |
| **Streams** | Client → xmtpd node (persistent connection) | ReplicationApi |

**Key Components:**

- **Gateway**: Entry point for write operations, handles authentication and routing
- **xmtpd nodes**: Decentralized network nodes that store and replicate messages
- **MultiNodeClient**: Automatically selects fastest node for reads

---

### **D14n gRPC Endpoints**

The d14n backend exposes endpoints through two gRPC services:

#### **PayerApi (Write Operations)**

> **Source:** [`crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs`](crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs)

| Endpoint                 | gRPC Path                                                | Purpose                                          |
| ------------------------ | -------------------------------------------------------- | ------------------------------------------------ |
| `PublishClientEnvelopes` | `/xmtp.xmtpv4.payer_api.PayerApi/PublishClientEnvelopes` | Publish messages, key packages, identity updates |
| `GetNodes`               | `/xmtp.xmtpv4.payer_api.PayerApi/GetNodes`               | Get list of available xmtpd nodes                |

#### **ReplicationApi (Read Operations)**

> **Source:** [`crates/xmtp_api_d14n/src/endpoints/d14n/`](crates/xmtp_api_d14n/src/endpoints/d14n/)

| Endpoint             | gRPC Path                                                    | Purpose                               |
| -------------------- | ------------------------------------------------------------ | ------------------------------------- |
| `QueryEnvelopes`     | `/xmtp.xmtpv4.message_api.ReplicationApi/QueryEnvelopes`     | Query messages by topic and cursor    |
| `GetNewestEnvelope`  | `/xmtp.xmtpv4.message_api.ReplicationApi/GetNewestEnvelope`  | Get latest message for topics         |
| `SubscribeEnvelopes` | `/xmtp.xmtpv4.message_api.ReplicationApi/SubscribeEnvelopes` | Stream real-time messages             |
| `GetInboxIds`        | `/xmtp.xmtpv4.message_api.ReplicationApi/GetInboxIds`        | Resolve wallet addresses to inbox IDs |

---

### **Topic System**

Topics are prefixed byte arrays that identify message streams.

> **Source:** [`crates/xmtp_proto/src/types/topic.rs`](crates/xmtp_proto/src/types/topic.rs)

```rust
// crates/xmtp_proto/src/types/topic.rs:14
pub enum TopicKind {
    GroupMessagesV1 = 0,    // Group messages (commits + application)
    WelcomeMessagesV1 = 1,  // Welcome messages for installations
    IdentityUpdatesV1 = 2,  // Identity association updates
    KeyPackagesV1 = 3,      // Key packages for installations
}

// Topic format: [kind_byte][identifier_bytes]
// Examples:
// - Group messages:  [0x00][group_id (32 bytes)]
// - Welcome messages: [0x01][installation_id (32 bytes)]
// - Identity updates: [0x02][inbox_id_bytes]
// - Key packages:     [0x03][installation_id (32 bytes)]
```

**Topic Construction:**

```rust
// crates/xmtp_proto/src/types/topic.rs:84-107
Topic::new_group_message(group_id)      // GroupMessagesV1
Topic::new_welcome_message(installation_id)  // WelcomeMessagesV1
Topic::new_identity_update(inbox_id)    // IdentityUpdatesV1
Topic::new_key_package(installation_id) // KeyPackagesV1
```

---

### **Request/Response Types**

#### **PublishClientEnvelopes**

> **Source:** [`crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs`](crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs)

```rust
// Request structure
pub struct PublishClientEnvelopes {
    envelopes: Vec<ClientEnvelope>,
}

// ClientEnvelope contains:
// - aad: AuthenticatedData (target_topic, depends_on cursor)
// - payload: GroupMessage | WelcomeMessage | KeyPackage | IdentityUpdate
```

**Request Parameters:**

| Parameter          | Type                  | Description                     |
| ------------------ | --------------------- | ------------------------------- |
| `envelopes`        | `Vec<ClientEnvelope>` | Messages to publish             |
| `aad.target_topic` | `Vec<u8>`             | Topic bytes (kind + identifier) |
| `aad.depends_on`   | `Option<Cursor>`      | Causal dependency for ordering  |

#### **QueryEnvelope / QueryEnvelopes**

> **Source:** [`crates/xmtp_api_d14n/src/endpoints/d14n/query_envelopes.rs`](crates/xmtp_api_d14n/src/endpoints/d14n/query_envelopes.rs)

```rust
// Single topic query
pub struct QueryEnvelope {
    topics: Vec<Topic>,
    last_seen: GlobalCursor,  // Vector clock position
    limit: u32,
}

// Batch query
pub struct QueryEnvelopes {
    envelopes: EnvelopesQuery,  // topics, originator_node_ids, last_seen
    limit: u32,
}
```

**Request Parameters:**

| Parameter   | Type           | Description                                     |
| ----------- | -------------- | ----------------------------------------------- |
| `topics`    | `Vec<Topic>`   | Topics to query                                 |
| `last_seen` | `GlobalCursor` | Vector clock cursor (node_id → sequence_id)     |
| `limit`     | `u32`          | Max messages to return (default: MAX_PAGE_SIZE) |

#### **GetNewestEnvelopes**

> **Source:** [`crates/xmtp_api_d14n/src/endpoints/d14n/get_newest_envelopes.rs`](crates/xmtp_api_d14n/src/endpoints/d14n/get_newest_envelopes.rs)

```rust
pub struct GetNewestEnvelopes {
    topics: Vec<Vec<u8>>,  // Topic bytes
}
// Returns latest envelope for each topic (or null if none)
```

#### **SubscribeEnvelopes**

> **Source:** [`crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_envelopes.rs`](crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_envelopes.rs)

```rust
pub struct SubscribeEnvelopes {
    topics: Vec<Topic>,
    last_seen: Option<GlobalCursor>,
    originators: Vec<OriginatorId>,
}
// Opens streaming connection for real-time messages
```

#### **GetInboxIds**

> **Source:** [`crates/xmtp_api_d14n/src/endpoints/d14n/get_inbox_ids.rs`](crates/xmtp_api_d14n/src/endpoints/d14n/get_inbox_ids.rs)

```rust
pub struct GetInboxIds {
    addresses: Vec<String>,  // Ethereum addresses
    passkeys: Vec<String>,   // Passkey identifiers
}
// Returns mapping of addresses → inbox_ids
```

---

### **Client Architecture**

#### **D14nClient**

The primary client for d14n operations:

> **Source:** [`crates/xmtp_api_d14n/src/queries/d14n/client.rs`](crates/xmtp_api_d14n/src/queries/d14n/client.rs)

```rust
// crates/xmtp_api_d14n/src/queries/d14n/client.rs:10
pub struct D14nClient<C, Store> {
    client: C,                          // HTTP/gRPC client
    cursor_store: Store,                // Tracks message cursors
    scw_verifier: Arc<MultiSmartContractSignatureVerifier>,
}
```

#### **ReadWriteClient (Middleware)**

Separates read and write operations to different backends:

> **Source:** [`crates/xmtp_api_d14n/src/middleware/read_write_client/client.rs`](crates/xmtp_api_d14n/src/middleware/read_write_client/client.rs)

```rust
// crates/xmtp_api_d14n/src/middleware/read_write_client/client.rs:17
pub struct ReadWriteClient<Read, Write> {
    read: Read,           // MultiNodeClient for reads
    write: Write,         // Gateway client for writes
    filter: String,       // Pattern to match write endpoints
}

// Filter: "xmtp.xmtpv4.payer_api.PayerApi"
// - Paths matching filter → write client (Gateway)
// - All other paths → read client (MultiNode)
```

#### **MultiNodeClient**

Automatically selects the fastest xmtpd node for reads:

> **Source:** [`crates/xmtp_api_d14n/src/middleware/multi_node_client/client.rs`](crates/xmtp_api_d14n/src/middleware/multi_node_client/client.rs)

```rust
// crates/xmtp_api_d14n/src/middleware/multi_node_client/client.rs:10
pub struct MultiNodeClient {
    gateway_client: GrpcClient,         // Gateway for GetNodes
    inner: OnceCell<GrpcClient>,        // Fastest node (lazy init)
    timeout: Duration,
    node_client_template: ClientBuilder,
}

// On first request:
// 1. Call GetNodes on gateway
// 2. Health check all returned nodes in parallel
// 3. Select fastest responding node
// 4. Cache selected node for all future requests
```

#### **ClientBundle (Builder)**

High-level builder for constructing the client stack:

> **Source:** [`crates/xmtp_api_d14n/src/queries/client_bundle.rs`](crates/xmtp_api_d14n/src/queries/client_bundle.rs)

```rust
ClientBundle::builder()
    .gateway_host("gateway.xmtp.org")  // D14n gateway for writes
    .app_version(version)
    .auth_callback(callback)           // For authenticated writes
    .is_secure(true)
    .readonly(false)
    .build()
```

---

### **CursorStore**

Tracks message positions for resumable queries and streams:

> **Source:** [`crates/xmtp_api_d14n/src/protocol/traits/cursor_store.rs`](crates/xmtp_api_d14n/src/protocol/traits/cursor_store.rs)

```rust
// crates/xmtp_api_d14n/src/protocol/traits/cursor_store.rs:48
pub trait CursorStore {
    // Compute minimum cursor across topics (for catching up)
    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, _>;

    // Get highest cursor for a topic
    fn latest(&self, topic: &Topic) -> Result<GlobalCursor, _>;

    // Get per-originator cursors
    fn latest_per_originator(&self, topic: &Topic, originators: &[&OriginatorId])
        -> Result<GlobalCursor, _>;

    // Find dependencies for message ordering
    fn find_message_dependencies(&self, hashes: &[&[u8]])
        -> Result<HashMap<Vec<u8>, Cursor>, _>;

    // Store orphaned envelopes (messages with unmet dependencies)
    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), _>;

    // Resolve children when dependency arrives
    fn resolve_children(&self, cursors: &[Cursor])
        -> Result<Vec<OrphanedEnvelope>, _>;
}
```

---

### **XmtpMlsClient Trait Implementation**

The d14n client implements the `XmtpMlsClient` trait, translating high-level operations to d14n endpoints:

> **Source:** [`crates/xmtp_api_d14n/src/queries/d14n/mls.rs`](crates/xmtp_api_d14n/src/queries/d14n/mls.rs)

```rust
// crates/xmtp_proto/src/api_client.rs:103
pub trait XmtpMlsClient {
    async fn upload_key_package(&self, request: UploadKeyPackageRequest) -> Result<(), _>;
    async fn fetch_key_packages(&self, request: FetchKeyPackagesRequest) -> Result<_, _>;
    async fn send_group_messages(&self, request: SendGroupMessagesRequest) -> Result<(), _>;
    async fn send_welcome_messages(&self, request: SendWelcomeMessagesRequest) -> Result<(), _>;
    async fn query_group_messages(&self, group_id: GroupId) -> Result<Vec<GroupMessage>, _>;
    async fn query_welcome_messages(&self, installation_key: InstallationId) -> Result<Vec<WelcomeMessage>, _>;
    // ...
}
```

**Operation Mapping (D14n):**

| Client Method                | D14n Endpoint            | Topic Kind          |
| ---------------------------- | ------------------------ | ------------------- |
| `upload_key_package`         | `PublishClientEnvelopes` | `KeyPackagesV1`     |
| `fetch_key_packages`         | `GetNewestEnvelopes`     | `KeyPackagesV1`     |
| `send_group_messages`        | `PublishClientEnvelopes` | `GroupMessagesV1`   |
| `send_welcome_messages`      | `PublishClientEnvelopes` | `WelcomeMessagesV1` |
| `query_group_messages`       | `QueryEnvelope`          | `GroupMessagesV1`   |
| `query_welcome_messages`     | `QueryEnvelope`          | `WelcomeMessagesV1` |
| `subscribe_group_messages`   | `SubscribeEnvelopes`     | `GroupMessagesV1`   |
| `subscribe_welcome_messages` | `SubscribeEnvelopes`     | `WelcomeMessagesV1` |

---

### **Client Lifecycle: Sending a Message**

```text
┌─────────────────────────────────────────────────────────────────┐
│                    CLIENT: Send Message                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 1. MlsGroup::send_message(content)                              │
│    Source: crates/xmtp_mls/src/groups/mod.rs                    │
│    - Encode content to EncodedContent                           │
│    - Queue SendMessage intent locally                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 2. MlsGroup::publish_intents()                                  │
│    Source: crates/xmtp_mls/src/groups/mls_sync.rs:2460          │
│    - Load intent from database                                  │
│    - Create MLS ApplicationMessage                              │
│    - TLS serialize the message                                  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 3. prepare_group_messages()                                     │
│    - Create GroupMessageInput with payload                      │
│    - Include sender_hmac for verification                       │
│    - Set should_push flag                                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 4. api.send_group_messages(request)                             │
│    Source: crates/xmtp_api_d14n/src/queries/d14n/mls.rs:89      │
│                                                                 │
│    D14nClient implementation:                                   │
│    a. Compute SHA256 hash of each message                       │
│    b. Find message dependencies from cursor_store               │
│    c. Create ClientEnvelope with:                               │
│       - aad.target_topic = [0x00][group_id]                     │
│       - aad.depends_on = cursor of dependency                   │
│       - payload = GroupMessageInput                             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 5. PublishClientEnvelopes → Gateway                             │
│    Endpoint: /xmtp.xmtpv4.payer_api.PayerApi/PublishClientEnvelopes
│    Source: crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs
│                                                                 │
│    Gateway:                                                     │
│    a. Validate envelope format                                  │
│    b. Forward to xmtpd node                                     │
│    c. Node assigns sequence_id and originator_id                │
│    d. Node replicates to other nodes                            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 6. Update intent state: ToPublish → Published                   │
│    Source: crates/xmtp_mls/src/groups/mls_sync.rs:2378          │
└─────────────────────────────────────────────────────────────────┘
```

---

### **Client Lifecycle: Receiving Messages**

```text
┌─────────────────────────────────────────────────────────────────┐
│                   CLIENT: Sync Messages                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 1. MlsGroup::receive()                                          │
│    Source: crates/xmtp_mls/src/groups/mls_sync.rs:2129          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 2. MlsStore::query_group_messages(group_id)                     │
│    Source: crates/xmtp_mls/src/mls_store.rs:78                  │
│    - Calls api.query_group_messages()                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 3. D14nClient::query_group_messages(group_id)                   │
│    Source: crates/xmtp_api_d14n/src/queries/d14n/mls.rs:139     │
│                                                                 │
│    a. Create topic: TopicKind::GroupMessagesV1.create(group_id) │
│    b. Get last seen cursor from cursor_store                    │
│    c. Build QueryEnvelope request                               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 4. QueryEnvelope → xmtpd Node                                   │
│    Endpoint: /xmtp.xmtpv4.message_api.ReplicationApi/QueryEnvelopes
│    Source: crates/xmtp_api_d14n/src/endpoints/d14n/query_envelopes.rs
│                                                                 │
│    Request:                                                     │
│    {                                                            │
│      topics: [[0x00, group_id...]],                             │
│      last_seen: { node_id_to_sequence_id: {100: 12345} },       │
│      limit: 100                                                 │
│    }                                                            │
│                                                                 │
│    Response: List of OriginatorEnvelope                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 5. Extract GroupMessage from OriginatorEnvelope                 │
│    Source: crates/xmtp_api_d14n/src/protocol/extractors/group_messages.rs
│                                                                 │
│    Envelope structure (nested):                                 │
│    OriginatorEnvelope                                           │
│      └─ UnsignedOriginatorEnvelope                              │
│           ├─ originator_sequence_id (cursor)                    │
│           ├─ originator_node_id                                 │
│           ├─ originator_ns (timestamp)                          │
│           └─ PayerEnvelope                                      │
│                └─ ClientEnvelope                                │
│                     ├─ aad (topic, depends_on)                  │
│                     └─ payload (GroupMessageInput)              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 6. Order messages (causal ordering)                             │
│    Source: crates/xmtp_api_d14n/src/queries/stream/ordered.rs   │
│                                                                 │
│    - Check depends_on cursor                                    │
│    - Hold message if dependency not yet received (orphan)       │
│    - Release when dependency arrives                            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 7. process_messages()                                           │
│    Source: crates/xmtp_mls/src/groups/mls_sync.rs:2048          │
│                                                                 │
│    For each GroupMessage:                                       │
│    a. Check if already processed (cursor)                       │
│    b. Decrypt MLS message using group state                     │
│    c. Validate sender (check sender_hmac)                       │
│    d. Decode PlaintextEnvelope                                  │
│    e. Store in group_messages table                             │
│    f. Update cursor                                             │
└─────────────────────────────────────────────────────────────────┘
```

---

### **Client Lifecycle: Streaming Messages**

```text
┌─────────────────────────────────────────────────────────────────┐
│                 CLIENT: Stream Messages                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 1. XmtpMlsStreams::subscribe_group_messages(group_ids)          │
│    Source: crates/xmtp_api_d14n/src/queries/d14n/streams.rs:38  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 2. Create topics for each group_id                              │
│    topics = group_ids.map(|gid| TopicKind::GroupMessagesV1.create(gid))
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 3. Get lowest common cursor from cursor_store                   │
│    - For each topic, find the minimum sequence_id               │
│    - Result is the "catch-up" position                          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 4. SubscribeEnvelopes → xmtpd Node                              │
│    Endpoint: /xmtp.xmtpv4.message_api.ReplicationApi/SubscribeEnvelopes
│    Source: crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_envelopes.rs
│                                                                 │
│    Opens bidirectional gRPC stream                              │
│    Request:                                                     │
│    {                                                            │
│      topics: [[0x00, group_id_1...], [0x00, group_id_2...]],    │
│      last_seen: { node_id_to_sequence_id: {...} },              │
│    }                                                            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 5. Stream processing pipeline                                   │
│    Source: crates/xmtp_api_d14n/src/queries/stream/             │
│                                                                 │
│    XmtpStream (raw gRPC)                                        │
│         │                                                       │
│         ▼                                                       │
│    FlattenedStream (unpack SubscribeEnvelopesResponse)          │
│         │                                                       │
│         ▼                                                       │
│    OrderedStream (causal ordering with orphan handling)         │
│         │                                                       │
│         ▼                                                       │
│    TryExtractorStream<GroupMessageExtractor>                    │
│         │                                                       │
│         ▼                                                       │
│    Stream<Item = Result<GroupMessage, Error>>                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 6. Client receives GroupMessage, processes as in sync flow      │
└─────────────────────────────────────────────────────────────────┘
```

---

### **Originator IDs**

Originator IDs distinguish message sources for cursor tracking. In the d14n network, different message types are processed by different components:

> **Source:** [`crates/xmtp_configuration/src/common/d14n.rs`](crates/xmtp_configuration/src/common/d14n.rs)

```rust
// crates/xmtp_configuration/src/common/d14n.rs
pub struct Originators;

impl Originators {
    pub const MLS_COMMITS: u32 = 0;           // Strongly-ordered commits
    pub const INBOX_LOG: u32 = 1;             // Identity updates
    pub const APPLICATION_MESSAGES: u32 = 10; // User messages
    pub const WELCOME_MESSAGES: u32 = 11;     // Welcome messages
    pub const INSTALLATIONS: u32 = 13;        // Key packages
    pub const DEFAULT: u32 = 100;             // Default for local/tests
}
```

**Cursor Interpretation:**

| Originator             | ID  | Description                                          |
| ---------------------- | --- | ---------------------------------------------------- |
| `MLS_COMMITS`          | 0   | Commits from strongly-ordered log (blockchain)       |
| `INBOX_LOG`            | 1   | Identity updates (CreateInbox, AddAssociation, etc.) |
| `APPLICATION_MESSAGES` | 10  | User chat messages                                   |
| `WELCOME_MESSAGES`     | 11  | MLS Welcome messages                                 |
| `INSTALLATIONS`        | 13  | Key package uploads                                  |

---

### **GlobalCursor (Vector Clock)**

A GlobalCursor tracks position across multiple originators:

> **Source:** [`crates/xmtp_proto/src/types/global_cursor.rs`](crates/xmtp_proto/src/types/global_cursor.rs)

```rust
// Internal representation: HashMap<OriginatorId, SequenceId>
// Example:
{
    0: 1234,   // MLS_COMMITS: seen up to sequence 1234
    10: 5678,  // APPLICATION_MESSAGES: seen up to sequence 5678
    11: 100,   // WELCOME_MESSAGES: seen up to sequence 100
}
```

**Operations:**

- `lcc()` - Lowest Common Cursor: minimum sequence_id across all originators
- `max()` - Maximum sequence_id across all originators
- `get(originator_id)` - Get sequence_id for specific originator

---

## Key Source Files Reference

| Concept                 | Primary Source File                                                   |
| ----------------------- | --------------------------------------------------------------------- |
| **Identity**            |                                                                       |
| AssociationState        | `crates/xmtp_id/src/associations/state.rs`                            |
| MemberIdentifier        | `crates/xmtp_id/src/associations/member.rs`                           |
| IdentityUpdate/Actions  | `crates/xmtp_id/src/associations/association_log.rs`                  |
| Installation            | `crates/xmtp_id/src/associations/ident/installation.rs`               |
| **Groups**              |                                                                       |
| MlsGroup                | `crates/xmtp_mls/src/groups/mod.rs`                                   |
| GroupMembership         | `crates/xmtp_mls/src/groups/group_membership.rs`                      |
| PolicySet/Permissions   | `crates/xmtp_mls/src/groups/group_permissions.rs`                     |
| ValidatedCommit         | `crates/xmtp_mls/src/groups/validated_commit.rs`                      |
| GroupMetadata           | `crates/xmtp_mls_common/src/group_metadata.rs`                        |
| GroupMutableMetadata    | `crates/xmtp_mls_common/src/group_mutable_metadata.rs`                |
| **Network Messages**    |                                                                       |
| GroupMessage            | `crates/xmtp_proto/src/types/group_message.rs`                        |
| WelcomeMessage          | `crates/xmtp_proto/src/types/welcome_message.rs`                      |
| Cursor/GlobalCursor     | `crates/xmtp_proto/src/types/cursor.rs`, `global_cursor.rs`           |
| KeyPackage              | `crates/xmtp_mls/src/verified_key_package_v2.rs`                      |
| Topic                   | `crates/xmtp_proto/src/types/topic.rs`                                |
| **Local Storage**       |                                                                       |
| StoredGroup             | `crates/xmtp_db/src/encrypted_store/group.rs`                         |
| StoredGroupMessage      | `crates/xmtp_db/src/encrypted_store/group_message.rs`                 |
| StoredGroupIntent       | `crates/xmtp_db/src/encrypted_store/group_intent.rs`                  |
| RefreshState            | `crates/xmtp_db/src/encrypted_store/refresh_state.rs`                 |
| **Intents**             |                                                                       |
| Intent Types/Data       | `crates/xmtp_mls/src/groups/intents.rs`                               |
| Intent Queue            | `crates/xmtp_mls/src/groups/intents/queue.rs`                         |
| **Content Types**       |                                                                       |
| MessageBody/Decoded     | `crates/xmtp_mls/src/messages/decoded_message.rs`                     |
| EncodedContent          | `crates/xmtp_proto/src/gen/xmtp.mls.message_contents.rs`              |
| Content Codecs          | `crates/xmtp_content_types/src/`                                      |
| **Configuration**       |                                                                       |
| Extension IDs           | `crates/xmtp_configuration/src/common/metadata.rs`                    |
| Originator IDs          | `crates/xmtp_configuration/src/common/d14n.rs`                        |
| MLS Parameters          | `crates/xmtp_configuration/src/common/mls.rs`                         |
| **Client**              |                                                                       |
| Client                  | `crates/xmtp_mls/src/client.rs`                                       |
| ClientBuilder           | `crates/xmtp_mls/src/builder.rs`                                      |
| Identity                | `crates/xmtp_mls/src/identity.rs`                                     |
| MlsSync                 | `crates/xmtp_mls/src/groups/mls_sync.rs`                              |
| MlsStore                | `crates/xmtp_mls/src/mls_store.rs`                                    |
| **D14n API Layer**      |                                                                       |
| D14nClient              | `crates/xmtp_api_d14n/src/queries/d14n/client.rs`                     |
| D14n MLS Operations     | `crates/xmtp_api_d14n/src/queries/d14n/mls.rs`                        |
| D14n Identity Ops       | `crates/xmtp_api_d14n/src/queries/d14n/identity.rs`                   |
| D14n Streams            | `crates/xmtp_api_d14n/src/queries/d14n/streams.rs`                    |
| ClientBundle            | `crates/xmtp_api_d14n/src/queries/client_bundle.rs`                   |
| ReadWriteClient         | `crates/xmtp_api_d14n/src/middleware/read_write_client/client.rs`     |
| MultiNodeClient         | `crates/xmtp_api_d14n/src/middleware/multi_node_client/client.rs`     |
| CursorStore             | `crates/xmtp_api_d14n/src/protocol/traits/cursor_store.rs`            |
| **D14n Endpoints**      |                                                                       |
| PublishClientEnvelopes  | `crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs` |
| QueryEnvelopes          | `crates/xmtp_api_d14n/src/endpoints/d14n/query_envelopes.rs`          |
| GetNewestEnvelopes      | `crates/xmtp_api_d14n/src/endpoints/d14n/get_newest_envelopes.rs`     |
| SubscribeEnvelopes      | `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_envelopes.rs`      |
| GetInboxIds             | `crates/xmtp_api_d14n/src/endpoints/d14n/get_inbox_ids.rs`            |
| GetNodes                | `crates/xmtp_api_d14n/src/endpoints/d14n/get_nodes.rs`                |
| **Protocol/Extractors** |                                                                       |
| GroupMessageExtractor   | `crates/xmtp_api_d14n/src/protocol/extractors/group_messages.rs`      |
| WelcomeExtractor        | `crates/xmtp_api_d14n/src/protocol/extractors/welcomes.rs`            |
| ProtocolEnvelope        | `crates/xmtp_api_d14n/src/protocol/impls/protocol_envelopes.rs`       |
| **API Traits**          |                                                                       |
| XmtpMlsClient           | `crates/xmtp_proto/src/api_client.rs`                                 |
| XmtpIdentityClient      | `crates/xmtp_proto/src/api_client.rs`                                 |
| XmtpMlsStreams          | `crates/xmtp_proto/src/api_client.rs`                                 |

---

## Summary

This document describes the d14n (decentralized) architecture of XMTP MLS. The key features enabled by this architecture:

- **Decentralized network:** Messages replicated across xmtpd nodes with no single point of failure
- **Multi-device support:** Same inbox_id on many devices via MLS group membership
- **Forward secrecy:** Key rotation protects past messages
- **Post-compromise security:** Key rotation recovers from compromise
- **Decentralized identity:** Wallet signatures prove ownership
- **Flexible permissions:** Groups can have custom policies
- **Offline support:** Intents allow optimistic local operations
- **Scalability:** Cursor-based pagination and causal ordering for message streams
- **Real-time streaming:** Bidirectional gRPC streams for instant message delivery
