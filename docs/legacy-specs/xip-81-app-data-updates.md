---
xip: 81
title: App Data Updates
description: Replace GroupContextExtensions with AppDataUpdate proposals for delta-based mutable group state with per-component permissions.
author: Tyler Hawkes (@tylerhawkes)
status: Draft
type: Standards
category: Core
created: 2026-05-20
---

## Abstract

This XIP replaces GroupContextExtensions (GCE) with AppDataUpdate proposals for all mutable group state in XMTP. GCE requires replacing the entire extensions set on every update, which is bandwidth-inefficient, conflict-prone, and prevents granular permissions. AppDataUpdate proposals target individual components with delta updates, enabling concurrent non-conflicting changes and permission enforcement per component.

## Motivation

XMTP groups currently use GroupContextExtensions (GCE) proposals to update all mutable group state: membership, metadata, permissions, and application data. GCE has several fundamental limitations:

**Full-state replacement.** Every GCE proposal replaces the *entire* set of group context extensions. Updating a single metadata field (e.g., group name) requires re-serializing and transmitting membership, permissions, admin lists, and all other extensions. This is bandwidth-inefficient, especially for large groups.

**Conflict-prone.** Two concurrent GCE proposals cannot both succeed. If Alice updates the group name while Bob updates the group description, the second commit will replace the first's changes entirely. There is no merge strategy — last writer wins.

**Coupled state.** Unrelated concerns are bundled into a single update mechanism. A membership change (adding a member) requires carrying along metadata and permissions extensions unchanged. This coupling makes it difficult to reason about what changed in a commit and creates unnecessary validation complexity.

**No granular permissions.** The current permission system maps policies to specific operations (add member, update metadata field X), but the underlying transport treats all extensions as a single blob. There is no way to express "any member can update component A, but only admins can update component B" at the MLS extension level.

The MLS extensions draft (draft-ietf-mls-extensions) introduces `AppDataUpdate` proposals that target individual components in an `AppDataDictionary` with delta updates. This XIP leverages that mechanism to address all of the above limitations.

## Specification

The keywords "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

### 1. Proposal Support Foundation

This section describes the proposal-by-reference infrastructure that has already been implemented and serves as the foundation for this XIP.

### 1.1 Proposal Support Extension

A custom extension with ID `0xff05` (`PROPOSAL_SUPPORT_EXTENSION_ID`) is used in two contexts:

- **Leaf node capabilities**: When included in a key package's leaf node capabilities, it indicates the installation supports receiving standalone MLS proposals (not just commits).
- **Group context**: When present in the group's extensions, it indicates the group operates in proposal-by-reference mode exclusively.

The extension carries a `ProposalSupport` proto with a `version` field (currently `1`).

### 1.2 Capability Negotiation

Before enabling proposals on a group, the initiator MUST verify that all current members support the proposal extension:

1. Check leaf nodes in the local ratchet tree (the MLS group state containing all members' leaf nodes) for `PROPOSAL_SUPPORT_EXTENSION_ID` in capabilities.
2. If any local leaf node does not advertise support, fetch fresh key packages from the server.
3. If all key packages include the extension in capabilities, proposals MAY be enabled.
4. If any member does not support proposals, the group MUST remain in direct-commit mode.

When adding new members to a proposal-enabled group, the adder MUST verify the new member's key packages support `PROPOSAL_SUPPORT_EXTENSION_ID`. If new members do not support proposals, the proposal support extension MUST be removed from the group context and the group reverts to direct-commit mode.

### 1.3 Enable Proposals Flow

Enabling proposals is a bootstrapping operation. The group is not yet in proposal mode, so a standard MLS proposal-then-commit sequence is used with the existing direct-publish mechanism. The `ProposeGroupContextExtensions` intent explicitly bypasses the `proposals_enabled` check since it is the mechanism used to enable proposals in the first place.

To enable proposals on a group:

1. Verify all members support proposals (Section 1.2).
2. Build the new group context extensions containing the `ProposalSupport` extension and updated `RequiredCapabilities`.
3. Queue a `ProposeGroupContextExtensions` intent with the serialized extensions. This intent handler does NOT require proposals to already be enabled — it creates a standard MLS GCE proposal and publishes it directly.
4. Sync until the proposal intent resolves.
5. Re-verify member support has not changed during sync — incoming messages may have added members who don't support proposals.
6. Queue a `CommitPendingProposals` intent to commit the pending GCE proposal.
7. Sync until the commit intent resolves.
8. Verify the proposal support extension is present in the committed group state.

After this flow completes, the group is in proposal mode and all subsequent state updates use proposal-by-reference.

### 1.4 Proposal Validation

When receiving proposals, the following validation rules apply:

| Proposal Type | Validation |
| --- | --- |
| **Add** | Proposer MUST have `add_member` permission. DM groups allow adding the other DM participant regardless of policy. |
| **Remove** | Proposer MUST have `remove_member` permission. Super admins MUST NOT be removed. |
| **Update** | The new leaf node credential MUST match the proposer's identity. Prevents identity swaps. |
| **GroupContextExtensions** | Proposer MUST have permission for each changed field (metadata, admin lists, permissions). Only super admins MAY change permissions or super admin lists. |
| **PreSharedKey** | MUST be rejected (`UnsupportedProposalType`). |
| **ReInit** | MUST be rejected (`UnsupportedProposalType`). |
| **ExternalInit** | MUST be rejected (`UnsupportedProposalType`). |
| **SelfRemove** | MUST be rejected (`UnsupportedProposalType`). |
| **AppDataUpdate** | See Section 2.4 for validation. |
| **AppEphemeral** | MUST be rejected (`UnsupportedProposalType`). |
| **Custom** | MUST be rejected (`UnsupportedProposalType`). The `BatchProposal` (`0xff00`) carve-out described in Section 4 is deferred from v1. |

### 1.5 Membership Commit Flows

There are two flows for committing membership changes, depending on whether the caller directly updates membership or proposes changes for a later commit.

#### 1.5.1 Batched Flow (UpdateGroupMembership)

When `add_members` or `remove_members` is called with proposals enabled, all messages are published atomically in a single `send_group_messages` call:

1. Create Add/Remove proposals for the membership changes.
2. Analyze the proposals to determine updated membership.
3. Build an updated `GroupMembership` extension with new inbox IDs and sequence IDs.
4. Create a GCE proposal with the updated membership extension.
5. Create a commit consuming all pending proposals (including the just-created GCE).
6. Publish the Add/Remove proposals, GCE proposal, and commit together in one call.

When the commit is received back from the network, the staged commit is merged. No re-queuing is needed.

#### 1.5.2 Two-Phase Flow (ProposeMemberUpdate + CommitPendingProposals)

When membership changes are proposed separately and committed later:

**Phase 1 (Propose):**

1. Create Add/Remove proposals for the membership changes.
2. Publish only the proposals. No GCE proposal or commit is created at this stage.

**Phase 2 (Commit):**

1. Receive the Add/Remove proposals back from the network.
2. Re-queue the `CommitPendingProposals` intent to `ToPublish`.
3. Analyze the pending proposals to determine updated membership.
4. Build an updated `GroupMembership` extension with new inbox IDs and sequence IDs.
5. Create a GCE proposal with the updated membership extension.
6. Create a commit consuming all pending proposals (including the new GCE).
7. Publish the GCE proposal and commit together in one call.

If a matching GCE proposal already exists in the pending proposals (compared by `members` field only), step 5 is skipped and the existing GCE is consumed by the commit.

### 2. AppDataDictionary and ComponentId Ranges

### 2.1 AppDataDictionary Extension

The `AppDataDictionary` is a group context extension (ExtensionType 6) defined in the MLS extensions draft. It stores a map of `ComponentId` (u16) to `ComponentData` (arbitrary bytes), maintained in ascending order by ComponentId.

All mutable group state that is currently stored in custom `Extension::Unknown` variants MUST be migrated into the `AppDataDictionary` as individual components. Each component stores its own serialized payload (see Section 2.6 for serialization requirements).

### 2.2 ComponentId Ranges

ComponentIds occupy the top half of the `u16` space and are split into three contiguous ranges:

| Range | Purpose |
| --- | --- |
| `0x8000-0xBFFF` | XMTP-allocated well-known components (this XIP) |
| `0xC000-0xFEFF` | Application-defined components, registered at runtime |
| `0xFF00-0xFFFF` | Reserved — hard-rejected, no graceful degradation |

Within each of the XMTP and application ranges, **immutable** components live at the *end* of the range, counting down. The shipped layout is:

- XMTP mutable: `0x8000` up.
- XMTP immutable: `0xBE00-0xBFFF` (the last 512 ids in the XMTP range, counting down).
- Application mutable: `0xC000` up.
- Application immutable: `0xFD00-0xFEFF` (the last 512 ids in the application range, counting down).

The contiguous-range / counting-down-immutables shape replaces the earlier proposal of permission-banded sub-ranges (super-admin / admin / any-member / inbox-id / installation-id). Per-component permissions are stored explicitly in a **component registry** (Section 2.2.1) rather than derived from the ComponentId itself, so a finer-grained range scheme is unnecessary.

**Immutable semantics:** Components in an immutable range MAY only be written when the ComponentId does not yet exist in the `AppDataDictionary`. Any `AppDataUpdate` proposal targeting an immutable ComponentId that already has a value MUST be rejected. `Remove` operations on immutable components MUST also be rejected.

#### 2.2.1 Component Registry (`0x8000`)

A single super-admin-only component at `ComponentId::COMPONENT_REGISTRY` (`0x8000`) holds per-component permission policies. The registry replaces both the range-default scheme and the separate `PermissionOverrideMap` originally proposed at `0x8801`.

Conceptually the registry is `Map<ComponentId, ComponentMetadata>`. `ComponentMetadata` carries the proto-level base policy (`AllowIfAdmin`, `AllowIfSuperAdmin`, etc.) and per-component flags.

Two ComponentIds have their permissions **enforced in code** rather than resolved through the registry — they're the ones the registry itself depends on to bootstrap:

- `0x8000` `COMPONENT_REGISTRY` — super-admin-only.
- `0x8001` `SUPER_ADMIN_LIST` — super-admin-only.

A third, `0x8002` `ADMIN_LIST`, is "constrained": its registry entry MAY only encode `AllowIfAdmin` or `AllowIfSuperAdmin` — narrower stored policies are rejected by `ComponentRegistry::validate_entry`.

For every other ComponentId, the registry entry is the source of truth. There is no range-based default — a ComponentId with no registry entry is denied by default.

Combined with the pre-commit evaluation rule (Section 2.4), this means a new component MUST be registered in a commit that **precedes** any commit that writes to it. The two operations cannot share a commit; if they did, the dependent write would be evaluated against the pre-commit registry (which lacks the entry) and rejected. Bootstrap is the only path that registers and writes in the same commit, and it uses a dedicated validator (Section 3.2 Step B).

#### 2.2.2 Application ComponentId Registration

Applications using the application range (`0xC000-0xFEFF`) need a mechanism to avoid ComponentId conflicts when multiple applications interact with the same XMTP group. The originally proposed on-chain registry is **deferred** — v1 ships without a global allocator.

The shipped path is per-group runtime registration: applications call into the SDK to register a component (id, type, permission policy) on a specific group; the SDK encodes that registration into the group's `COMPONENT_REGISTRY` (`0x8000`) on bootstrap or subsequent updates. Collision risk is per-group, and the registry itself rejects duplicate ids.

**Candidate refinement: 32-byte stable identifier per component.** A direction worth exploring for cross-group allocation: each application-range component carries a stable 32-byte identifier (e.g., a hash of the app domain or an explicit per-component handle) stored in the registry alongside the policy. Apps reference components by the 32-byte identifier in their write/read APIs; the SDK resolves it to the per-group 16-bit ComponentId via a registry lookup. Different groups can independently assign different 16-bit slots without affecting application code, and multiple apps can ship components without coordinating on the 16-bit namespace at all. See Open Questions for status.

See Rationale for alternative registration approaches considered.

### 2.3 Well-Known Component IDs

The shipped layout numbers well-known mutable components counting up from `0x8000` and immutable components counting down from `0xBFFF`. There are no permission-banded sub-ranges — permissions for each id live in the registry described in Section 2.2.1.

**Hardcoded (`0x8000-0x8002`, permissions enforced in code):**

| ComponentId | Name | Type | Description |
| --- | --- | --- | --- |
| `0x8000` | ComponentRegistry | `TlsMap<bytes, bytes>` | Per-component permission metadata (Section 2.2.1) |
| `0x8001` | SuperAdminList | `TlsSet<InboxId>` | Super admin inbox IDs |
| `0x8002` | AdminList | `TlsSet<InboxId>` | Admin inbox IDs (constrained policy) |

**Well-known mutable (`0x8003+`, permissions read from the registry):**

| ComponentId | Name | Type | Description |
| --- | --- | --- | --- |
| `0x8003` | GroupMembership | `TlsMap<InboxId, bytes>` | Inbox IDs → sequence IDs / failed installations |
| `0x8004` | GroupName | `String` | Human-readable group name |
| `0x8005` | GroupDescription | `String` | Group description |
| `0x8006` | GroupImageUrl | `String` | Group image URL |
| `0x8007` | MessageDisappearFromNs | `Bytes` | Disappearing-message start (ns) |
| `0x8008` | MessageDisappearInNs | `Bytes` | Disappearing-message TTL (ns) |
| `0x8009` | AppData | `String` | Application-specific data string |
| `0x800A` | MinimumSupportedProtocolVersion | `String` | XIP-72 protocol-version floor |
| `0x800B` | CommitLogSigner | `Bytes` | Per-group commit-log signing key |

**Well-known immutable (`0xBFFC-0xBFFF`, counting down):**

| ComponentId | Name | Type | Description |
| --- | --- | --- | --- |
| `0xBFFF` | ConversationType | (seed bytes) | Conversation type (Group, DM, Sync) |
| `0xBFFE` | CreatorInboxId | `InboxId` (TLS-encoded) | Inbox ID of the group creator |
| `0xBFFD` | DmMembers | `TlsSet<InboxId>` | DM participant inbox IDs |
| `0xBFFC` | OneshotMessage | (seed bytes) | Initial oneshot payload |

`PinnedFrameUrl` from the original draft is not present in the v1 shipped set; if added later, it slots into the well-known mutable range alongside the other URL-bearing components.

### 2.4 AppDataUpdate Proposal Validation

When receiving an `AppDataUpdate` proposal:

1. Verify the proposer is a current member of the group. All `AppDataUpdate` proposals MUST be rejected with `ActorNotMember` if the proposer is not a member, regardless of the target ComponentId.
2. Extract the `ComponentId` from the proposal.
3. If the id is in the reserved range (`0xFF00-0xFFFF`), reject unconditionally.
4. If the id is hardcoded (`COMPONENT_REGISTRY`, `SUPER_ADMIN_LIST`), apply the in-code policy (super-admin-only).
5. Otherwise, read the registry entry for the id from `COMPONENT_REGISTRY` (`0x8000`). If no entry exists, reject (deny-by-default for unregistered ids).
6. Apply the registry entry's policy against the proposer's role (membership / admin / super-admin).
7. If the id is immutable and already present in the `AppDataDictionary`, reject (immutable components cannot be modified or removed).
8. Reject with `InsufficientPermissions` if validation fails.

#### Pre-commit permission evaluation

All permission checks above MUST be evaluated against the **state of the group at the start of the commit**, never against state produced by proposals inside the same commit. Specifically:

- The `COMPONENT_REGISTRY` (`0x8000`) value used in step 5 MUST be the pre-commit registry.
- The `SUPER_ADMIN_LIST` (`0x8001`) and `AdminList` (`0x8002`) values used in step 6 MUST be the pre-commit lists.
- The "already present" check in step 7 MUST be against the pre-commit dictionary.

This is the property that prevents privilege-escalation within a commit. A commit that, in its proposal set, *both* mutates `COMPONENT_REGISTRY` (or `SUPER_ADMIN_LIST` / `ADMIN_LIST`) *and* writes a component whose authorization depends on that mutation MUST be rejected — the dependent write is evaluated against the *old* registry/admin state and fails. As an operational consequence: any change that grants permission and any write that exercises that permission MUST land in **separate commits**.

The single exception is the bootstrap commit (Section 3.2 Step B), which legitimately needs to "register a component and write its initial value in the same commit." Bootstrap commits route through a dedicated validator that authorizes the initial registry-plus-write atomically; ordinary post-migration commits do not.

#### Unknown-component tolerance

Receivers MUST accept opaque `Update` / `Remove` payloads for unknown ComponentIds in the XMTP and application ranges (`0x8000-0xFEFF`) — registry-policy validation still applies, but the per-component decode hook is skipped. This is what lets older clients survive seeing a newer well-known component without forking the dictionary. The reserved range (`0xFF00-0xFFFF`) is still hard-rejected.

### 2.5 AppDataUpdate Processing

When processing a commit containing `AppDataUpdate` proposals, the receiver MUST:

1. Build an `AppDataDictionaryUpdater` from the current group state.
2. For each `AppDataUpdate` proposal in the commit:
    - If `Update(data)`: set the component data for the given ComponentId.
    - If `Remove`: remove the component data for the given ComponentId.
3. Call `process_unverified_message_with_app_data_updates` with the computed updates.
4. Persist the updated component values to local storage.

### 2.6 Deterministic Serialization

The `AppDataDictionary` is a GroupContext extension. The serialized GroupContext is an input to the MLS key schedule (epoch secret derivation), transcript hash, and tree hash. All group members MUST produce byte-identical GroupContext serializations after processing a commit, or their derived keys will diverge and they will be unable to decrypt messages.

The shipped wire format is **TLS codec** throughout — both for delta-based components (sorted, length-prefixed vectors) and for simple value components. Protobuf is not used for `ComponentData`. This avoids a second serialization choice per component and ensures byte-identical reconstruction across implementations.

Two implementation-level details that are part of the wire format:

1. **`ComponentId` uses QUIC variable-length integer encoding** (RFC 9000 §16) rather than a fixed `u16`. Current ids (`0x8000-0xFFFF`) take 4 bytes. The variable-length form is forward-compatible — the underlying type can grow beyond `u16` without breaking the wire format.
2. **Inbox IDs use the versioned `InboxId` TLS encoding** (`varint(version) || 32-byte payload`), never raw UTF-8 hex bytes. Every component whose payload contains one or more inbox ids (`SuperAdminList`, `AdminList`, `GroupMembership`, `DmMembers`, `CreatorInboxId`) routes through this encoding. The 33-byte versioned form is the source of truth on the wire and is decoded back to a hex string only at the legacy-host-API boundary.

The discussion of alternatives (canonical protobuf, bincode, rkyv, ASN.1 DER) is preserved for context but is no longer load-bearing — TLS codec is the only format used in the shipped implementation.

### 3. Eliminating GroupContextExtensions

This section builds on [XIP-72](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-72-protocol-version-client-enforcement.md), which defines `minimum_supported_protocol_version` as mutable metadata and specifies the client enforcement behavior (pausing groups when the version requirement is not met). The mechanisms below extend XIP-72 with tools for assessing migration readiness *before* bumping the min version.

### 3.1 Protocol Version Signaling

Before initiating a migration, clients need to determine what percentage of a group's members will be paused by a version bump (as defined in XIP-72). Two complementary mechanisms provide this signal:

#### Leaf Node Extension (local fast-path)

A new leaf node extension broadcasts the client's supported protocol version in its key package:

```rust
struct ProtocolVersionExtension {
    major: u16,
    minor: u16,
    patch: u16,
}
```

This extension is included in key packages and leaf nodes. When a client upgrades to a new protocol version, it MUST update its leaf node in all groups via an MLS Update proposal to broadcast the new version. Clients can then inspect the ratchet tree locally to check version support across the group without any network requests.

The leaf node data may lag behind the server-side data if a client has upgraded and uploaded a new key package but has not yet updated its leaf node in every group. The server query (below) provides the authoritative count.

#### Server-Side Query (authoritative)

The server tracks the protocol version for each installation based on the latest uploaded key package. Clients query the server for the version distribution across a group's members:

```protobuf
rpc GetGroupVersionSupport(GetGroupVersionSupportRequest) returns (GetGroupVersionSupportResponse);

message GetGroupVersionSupportRequest {
    bytes group_id = 1;
    string target_version = 2;  // The version to check support for
}

message GetGroupVersionSupportResponse {
    uint32 total_members = 1;
    uint32 members_supporting = 2;    // Members with at least one installation at target_version
    uint32 members_not_supporting = 3; // Members with no installations at target_version
}
```

This is the authoritative source of truth since the server sees all key package uploads. Clients SHOULD use this query before initiating a migration to understand the impact.

#### Migration Readiness

A client SHOULD check migration readiness before bumping the min group version:

1. Query the server for version support at the target version.
2. If the percentage of supporting members meets a configurable threshold (e.g., 80%, 100%), proceed with migration.
3. If not, defer migration and optionally notify the user that some members will be paused.

The threshold is an application-level policy decision — some applications may require 100% support before migrating, while others may accept pausing a minority of members.

### 3.2 Transition via Min Group Version

The preferred migration mechanism uses the existing min group version functionality. When a group's minimum protocol version is bumped to a version that requires AppDataUpdate support:

1. Clients that support the new version continue processing normally.
2. Clients on older versions **pause** — they stop processing messages for the group but do not fork or lose state. The group appears paused until the client upgrades.
3. When an old client upgrades, it resumes processing and catches up on all messages it missed while paused, including AppDataUpdate commits.

This avoids the risk of forking that capability-based negotiation introduces, and does not require checking every member's key packages before transitioning.

**Migration flow (two-step bootstrap):**

The single-GCE-commit form originally proposed leaves a window where an old peer would receive a commit that *both* bumps the version floor *and* strips the legacy mutable-metadata / membership / permissions extensions. An old peer cannot decode that commit cleanly: by the time it observes the new floor and decides to pause, the legacy state it relies on for catch-up has already been removed.

The shipped flow splits the migration into two commits:

**Step A — legacy GCE bump.** Issue a standard GCE commit that *only* writes `minimum_supported_protocol_version` (XIP-72) into the legacy mutable-metadata extension. The target version is taken from `EnableProposalsOptions::min_version` (default: `xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION`, currently `"1.11.0"`). The bump is semver-aware and idempotent — if the floor is already at or above the target the step is skipped, so concurrent migrators don't downgrade each other.

**Step B — proposal-then-commit bootstrap.** Run the standard MLS proposal-then-commit sequence (Section 1.3) to:

1. Add the `AppDataDictionary` extension populated from the legacy state at the ComponentIds in Section 2.3.
2. Add the `ProposalSupport` extension and updated `RequiredCapabilities`.
3. Remove the legacy `MutableMetadata`, `GroupMembership`, and `GroupPermissions` `Extension::Unknown` variants.

Old peers process Step A first, see the version mismatch in `validate_one_commit`, land in `paused_for_version`, and never process Step B's legacy-extension-stripping commit. Step B then runs across the upgraded peer set.

A pre-flight `all_members_support_proposals` check still runs before Step A unless `EnableProposalsOptions::force` is set; `force` is for post-d14n environments where capability advertisement is universal.

After migration, the group MUST reject any further GCE proposals. All state updates MUST use `AppDataUpdate` proposals — including subsequent bumps to the version floor, which target `MinimumSupportedProtocolVersion` (`0x800A`) directly. A migrated peer raising the floor via an AppDataUpdate to `0x800A` pauses lower-version peers identically to the legacy GCE form.

### 3.3 Alternative: Capability Negotiation

As an alternative to min group version, groups MAY use capability-based negotiation to transition only when all members already support AppDataUpdate:

1. All members' key packages advertise support for the `AppDataDictionary` extension type.
2. All members' key packages advertise support for the `AppDataUpdate` proposal type.
3. The group has proposal support enabled (Section 1).

When all three criteria are met, any member MAY initiate the same Step B migration procedure (Section 3.2) without first running Step A's legacy version-floor bump.

This approach avoids pausing any members but requires waiting for every member to upgrade before the group can transition. It is suitable for groups where pausing members is unacceptable, but delays the migration until full adoption. The shipped implementation does not use this path on its own — it always runs Step A's idempotent floor bump as well, so pre-1.11 peers pause if they ever rejoin a group whose other members are already migrating.

### 3.4 Post-Migration State Updates

After migration, the intent handlers change as follows:

| Intent | Before Migration | After Migration |
| --- | --- | --- |
| MetadataUpdate | GCE commit with full extensions | AppDataUpdate proposal for the relevant ComponentId (`0x8004`–`0x8009`) |
| UpdateAdminList | GCE commit with full extensions | AppDataUpdate proposal for `AdminList` (`0x8002`) |
| UpdatePermission | GCE commit with full extensions | AppDataUpdate proposal for `ComponentRegistry` (`0x8000`) |
| UpdateGroupMembership | GCE proposal + commit | AppDataUpdate proposal for `GroupMembership` (`0x8003`) plus Add/Remove proposals |
| Bump minimum protocol version | GCE commit | AppDataUpdate proposal for `MinimumSupportedProtocolVersion` (`0x800A`) |

**Wire shape: standalone proposal + commit.** Migrated sender intents publish two messages per metadata update: a standalone `AppDataUpdate` proposal first, then a commit that consumes the proposal store. This is the same MLS proposal-by-reference shape used elsewhere — it is *not* the inline-bundled GCE-proposal-plus-commit pattern from the legacy path.

**Lazy batching.** The commit issued by `stage_app_data_propose_and_commit` has no precondition on the pending proposal store. It sweeps every existing pending proposal — concurrent `AppDataUpdate`s, leaf-node `Update`s, `Add`/`Remove`/`SelfRemove`, PSK, etc. — into the same commit body. Producers of those pending proposals already accepted "ride into the next commit" by leaving them pending; MLS freezes message-send while proposals are pending regardless of which commit ultimately consumes them. The dictionary-update step in receivers iterates only the `AppDataUpdate` proposals; non-AppData proposals ride natively through OpenMLS.

### 4. Atomic Batching via Custom Proposal

> **Status: deferred from v1.** The shipped implementation does not include `BatchProposal`. Atomic batching of `AppDataUpdate` with `Add`/`Remove` is handled by lazy batching at commit time (Section 3.4) and by the standalone-proposal-plus-commit shape. The design below is preserved as future work.

### 4.1 Problem Statement

Several group operations require multiple state changes that MUST be applied atomically. For example, adding a member requires both an MLS Add proposal and an update to the GroupMembership component. Currently, this is achieved by creating multiple proposals and a commit in a single `send_group_messages` call, but there is no guarantee the network delivers them atomically.

### 4.2 Recommended: Custom Batch Proposal

Define a custom MLS proposal type (`ProposalType::Unknown(0xff00)`) that wraps a list of standard MLS proposals:

```rust
struct BatchProposal {
    proposals: Vec<Proposal>,
}
```

The `BatchProposal` contains standard MLS `Proposal` values (Add, Remove, AppDataUpdate, GroupContextExtensions, etc.). This avoids introducing new operation types — each inner proposal uses the same data structures and validation logic as if it were a standalone proposal.

**Processing:**

1. When a `BatchProposal` is received, the receiver MUST unbatch it into its constituent proposals.
2. Each inner proposal MUST be validated independently using the existing proposal validation rules (Section 1.4, Section 2.4).
3. If any inner proposal fails validation, the entire batch MUST be rejected.
4. The committer applies all inner proposals when committing, using the same application logic as standalone proposals.

**Semantics:**

- A `BatchProposal` MUST be treated as atomic: either all inner proposals are valid and applied, or none are.
- Validation reuses existing per-proposal-type checks — no new validation code paths are needed.
- A single `BatchProposal` results in a single MLS proposal message on the network.
- Inner proposals MUST be applied in the order they appear in the `proposals` vector.

**Benefits:**

- Single proposal reference for intent tracking.
- Single network message reduces latency and ensures atomicity.
- Works within MLS semantics — no network protocol changes required.
- Supports any combination of proposal types (Add, Remove, AppDataUpdate, etc.).
- Reuses all existing validation and application logic — no new code paths for individual operations.

See Rationale for alternative batching approaches considered.

### 5. Application Data API

> **Status: not yet shipped.** v1 retains the single-component `app_data` string at `0x8009` (write-only via `update_app_data`, full-replacement semantics). The KeyValue API and `KeyValueDelta` mutations described below are planned but not yet implemented.

### 5.1 Motivation

The current `app_data` API exposes a single opaque string per group. Applications like Convos use this to store structured data (e.g., display name mappings per member). Today, any change requires the application to:

1. Deserialize the entire `app_data` string.
2. Modify one entry (e.g., change a single user's display name).
3. Re-serialize the entire string.
4. Call `update_app_data()`, which triggers a GCE commit replacing all group extensions.

This is inefficient for structured data that changes frequently. With `AppDataUpdate` proposals, we can expose an API that sends only the delta — but the component's internal format must support delta application.

### 5.2 Key-Value Component Format

Application components that need granular updates SHOULD use a key-value map format within a single ComponentId. The component stores a sorted map of `[u8]` keys to `[u8]` values, serialized deterministically (see Section 2.6).

```rust
struct KeyValueComponent {
    entries: Vec<KeyValueEntry>,  // sorted by key, no duplicates
}

struct KeyValueEntry {
    key: Vec<u8>,
    value: Vec<u8>,
}
```

Delta operations are encoded in the `AppDataUpdate` proposal's `ComponentData` as a list of mutations:

```rust
struct KeyValueDelta {
    mutations: Vec<KeyValueMutation>,  // applied in order
}

enum KeyValueMutation {
    Insert { key: Vec<u8>, value: Vec<u8> },
    Update { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}
```

When processing an `AppDataUpdate` proposal for a key-value component:

1. Deserialize the current `ComponentData` as a `KeyValueComponent`.
2. Deserialize the update payload as a `KeyValueDelta`.
3. Apply each mutation in order:
    - `Insert`: add the key-value pair. MUST fail if the key already exists.
    - `Update`: replace the value for an existing key. MUST fail if the key does not exist.
    - `Delete`: remove the key. MUST fail if the key does not exist.
4. Re-serialize the `KeyValueComponent` deterministically (entries sorted by key).

Because all members apply the same delta to the same base state, and the serialization is deterministic, all members produce byte-identical `ComponentData` (Section 2.6).

### 5.3 Client API

The SDK exposes delta operations directly, so applications never need to deserialize or re-serialize the full component:

```rust
// Insert a new key-value pair
conversation.app_data_insert(component_id, key: &[u8], value: &[u8]).await?;

// Update an existing key's value
conversation.app_data_update(component_id, key: &[u8], value: &[u8]).await?;

// Delete a key
conversation.app_data_delete(component_id, key: &[u8]).await?;

// Read a single key (local, no network)
let value: Option<Vec<u8>> = conversation.app_data_get(component_id, key: &[u8])?;

// Read all entries (local, no network)
let entries: Vec<(Vec<u8>, Vec<u8>)> = conversation.app_data_entries(component_id)?;
```

Each write method creates an `AppDataUpdate` proposal containing only the `KeyValueDelta` for the requested mutation. Multiple mutations MAY be batched into a single `KeyValueDelta` within one `AppDataUpdate` proposal.

Bindings for Node.js, WASM, and mobile expose equivalent methods with platform-appropriate types (e.g., `Uint8Array` for WASM, `ByteArray` for Kotlin).

### 5.4 Example: Display Name Mappings

An application like Convos storing per-member display names would use a single ComponentId in the application range (e.g., `0xe000`):

```rust
// Set display name for a member
conversation.app_data_insert(0xe000, b"display:inbox_abc123", b"Alice").await?;

// Change display name
conversation.app_data_update(0xe000, b"display:inbox_abc123", b"Alicia").await?;

// Remove display name
conversation.app_data_delete(0xe000, b"display:inbox_abc123").await?;

// Read display name (local)
let name = conversation.app_data_get(0xe000, b"display:inbox_abc123")?;
```

Each operation sends only the mutation over the network — not the entire display name mapping for all members.

## Rationale

### Why a per-component registry instead of range-based defaults

The original draft proposed permission-banded sub-ranges (super-admin / admin / any-member / inbox-id / installation-id) with an optional `PermissionOverrideMap` for groups that needed to customize. The shipped design collapses both into a single per-component registry at `0x8000`.

Reasons for the collapse:

- The override map already had to be a super-admin-only component to bootstrap correctly. Once that's in place, the range defaults are a fallback path nobody actually wants to depend on for production permission decisions — every well-known component has a known policy that can just be stored explicitly.
- Range-banded ids burn id space. Splitting `0x8000-0xBFFF` into seven sub-ranges forced the well-known mutable components to live in narrow per-permission buckets. The shipped layout (counting up from `0x8000`, immutables counting down from `0xBFFF`) gives the full range to whichever components need it.
- A registry is the natural data structure for runtime-registered application components anyway. Application components in `0xC000-0xFEFF` need *some* per-id policy lookup; reusing that lookup for well-known ids removes one mechanism from the design.

`COMPONENT_REGISTRY` and `SUPER_ADMIN_LIST` are hardcoded to super-admin-only in code (the registry can't define its own permissions without circular bootstrap). `ADMIN_LIST` is constrained to admin-or-super-admin. Every other id reads its policy from the registry, with deny-by-default for unregistered ids.

### Why eliminate GCE entirely

Keeping GCE for some operations while using AppDataUpdate for others creates two code paths for state updates, complicates validation (must check both mechanisms), and leaves the conflict problem unsolved for GCE-managed state. A clean break simplifies the implementation and ensures all state updates benefit from delta semantics.

### Why custom proposal over network protocol change

A custom MLS proposal works within the existing MLS framing and network transport. It requires no changes to nodes, payers, or the `send_group_messages` API. The proposal is self-contained — validators can verify it using standard MLS proposal validation. Network protocol changes are more invasive and require coordinated deployment across infrastructure.

### Why one-time migration over dual-write

Dual-write (writing to both old extensions and AppDataDictionary during a transition period) doubles the bandwidth cost and requires maintaining both code paths indefinitely. A one-time migration is a clean cutover: once migrated, the old code paths can be removed. Mixed-version safety comes from the version-floor pause described in Section 3.2 — peers that don't support `AppDataUpdate` pause the group rather than fork, so the migration can be initiated by any single supporting member without waiting for unanimous adoption.

### Atomic batching alternatives

Three approaches were considered for atomic batching:

| Aspect | Custom BatchProposal | Batched GroupMessageInput | Commit-level batching |
| --- | --- | --- | --- |
| Network changes | None | Requires node + payer changes | None |
| Atomicity scope | Single proposal | Network-level delivery | Commit-level only |
| Proposal type coverage | Any (Add, Remove, AppDataUpdate, etc.) | Any | AppDataUpdate only |
| Intent tracking | Single reference | Single reference | Multiple hashes |
| Validation | Reuses existing per-type checks | Client must validate batch contents | Standard MLS |
| Risks | New custom proposal type | Malicious clients can mix proposal/app messages | No atomicity for proposals themselves |

**Batched GroupMessageInput** modifies `GroupMessageInput` to carry multiple MLS messages in a `repeated bytes batch_data` field. All entries are delivered atomically. The main drawback is requiring network protocol changes across all nodes and the payer service, and there is no way to enforce that only proposals (not application messages) are batched together.

**Commit-level batching** uses standard MLS semantics where multiple proposals in a single commit are applied atomically. No new types or protocol changes are needed, but proposals are sent as separate network messages before the commit, and it doesn't solve batching when Add/Remove and AppDataUpdate proposals originate from different proposers.

The **custom BatchProposal** is the form preserved in Section 4 as future work. It is deferred from v1 — the shipped implementation relies on commit-level batching plus the lazy-batching shape (Section 3.4) for the cases that have come up so far.

### Application ComponentId registration alternatives

| Aspect | On-chain registry | Hash-based namespace | First-write-wins |
| --- | --- | --- | --- |
| Collision risk | None (assigned ranges) | Low (hash collisions) | Per-group only |
| Infrastructure | Smart contract | None | None |
| Coordination | Global, permanent | Convention-based | Per-group |
| Multi-app groups | Safe | Mostly safe | Fragile |

**Hash-based namespace** derives a ComponentId sub-range from `hash(app_domain) mod range_size + range_start`. Deterministic and requires no infrastructure, but has a small risk of hash collisions with no detection mechanism.

**First-write-wins** uses no global registration — the first `AppDataUpdate` to a ComponentId in a group implicitly claims it. Simplest to implement but fragile in multi-app scenarios since two apps can independently choose the same ComponentId in different groups.

The **on-chain registry** is preserved as the target end state but is deferred from v1. v1 ships with **per-group explicit registration**: applications register a component (id, type, policy) into the per-group `COMPONENT_REGISTRY` (`0x8000`) before writing to it. Duplicate ids are rejected at registration time, so the collision surface is bounded to a single group. The shape is closer to "first-register-wins-per-group" than first-write-wins; cross-group coordination is left as an open question (Open Questions §5).

## Backward Compatibility

### Existing Groups

Groups without proposal support continue to operate in direct-commit mode using GCE. No changes are required for these groups until a member initiates migration.

### Mixed-Version Groups

When migration is initiated via min group version (Section 3.2), old clients pause the group rather than fork. They do not lose state and will resume processing after upgrading. This is the preferred approach because it allows migration to proceed without waiting for every member to upgrade.

If capability negotiation (Section 3.3) is used instead, the group MUST NOT migrate until all members support `AppDataUpdate`.

### New Groups

Newly created groups where all initial members support `AppDataUpdate` SHOULD be created with the `AppDataDictionary` from the start, skipping the migration step. The group creator populates the initial component values during group creation and sets the min protocol version to require AppDataUpdate support.

### Client Compatibility

Old clients that do not support `AppDataUpdate` cannot join post-migration groups because `RequiredCapabilities` will include the `AppDataUpdate` proposal type. These clients must upgrade before joining. Old clients already in the group at migration time are paused (not removed) and resume after upgrading.

### BatchProposal

Deferred from v1 (Section 4). When introduced in a later release, the same compatibility considerations apply: old clients reject unknown custom proposal types, so `BatchProposal` (`0xff00`) MUST only be used in groups where all members advertise support for it via `RequiredCapabilities`.

## Security Considerations

### Permission Escalation

Permission checks for every `AppDataUpdate` proposal MUST be enforced at proposal validation and again during commit processing — not deferred to the application layer. A malicious proposer could otherwise craft an `AppDataUpdate` targeting a super-admin-only ComponentId (e.g., `COMPONENT_REGISTRY` or `SUPER_ADMIN_LIST`). The validator MUST resolve the proposer's role against the pre-commit `SUPER_ADMIN_LIST` / `ADMIN_LIST` and the pre-commit registry entry for the target id, and reject any unauthorized update.

### Pre-Commit Permission Evaluation

As detailed in Section 2.4, all permission decisions are made against the pre-commit state of the group. A commit cannot grant a proposer permission and use that permission in the same commit — the dependent write evaluates against the old registry / admin lists and fails. This rules out an entire class of escalation attacks where an attacker tries to atomically rewrite the registry and immediately exercise the rewritten policy. The only exception is the bootstrap commit, which uses a dedicated validator (Section 3.2 Step B); ordinary commits MUST NOT bypass pre-commit evaluation.

### Ordering Attacks

The MLS extensions draft specifies that `AppDataUpdate` proposals MUST be ordered after `GroupContextExtensions` proposals within a commit. Implementations MUST enforce this ordering. Combined with the pre-commit evaluation rule above, this prevents a malicious committer from using a GCE (during the migration window) to overwrite AppDataDictionary state set by a prior AppDataUpdate, or from reordering `AppDataUpdate` proposals to make a dependent write appear authorized.

### Identity Verification

Proposer identity is derived from the MLS sender leaf index and verified against the group's ratchet tree. The `Update` proposal validation (Section 1.4) ensures credential consistency, preventing identity swaps via leaf node updates.

### Threat Model

**Malicious group member:** Could attempt to write to ComponentIds for which they lack permission. Mitigated by registry-policy enforcement at proposal reception and commit processing.

**Malicious committer:** Could attempt to bundle a registry mutation with a dependent write, or to reorder proposals within a commit to change semantics. Mitigated by pre-commit permission evaluation (Section 2.4) and MLS ordering constraints.

**Malicious node:** Could attempt to drop or reorder messages. AppDataUpdate proposals within a commit are atomic — partial application is not possible.

**Replay attacks:** MLS epoch-based state prevents replay of proposals from previous epochs. Each proposal is bound to the current group epoch.

## Reference Implementation

The proposal-by-reference foundation and the AppData migration are implemented in libxmtp on `main`. Key files:

- `crates/xmtp_mls_common/src/app_data/component_id.rs` — `ComponentId` definitions, range helpers (`is_xmtp_range`, `is_app_range`, `is_reserved`, `is_immutable`), QUIC vlen TLS codec.
- `crates/xmtp_mls_common/src/app_data/component_registry.rs` — `ComponentRegistry` at `COMPONENT_REGISTRY` (`0x8000`), `validate_entry`, registry-policy enforcement.
- `crates/xmtp_mls_common/src/app_data/registry_table.rs` — `WELL_KNOWN` dispatch table and `lookup_component`, plus the `well_known_count_tripwire` change-control test.
- `crates/xmtp_mls/src/groups/app_data/component_source.rs` — `read_component_bytes`, `read_post_commit_component_bytes`, unknown-component tolerance at the three receive-side sites.
- `crates/xmtp_mls/src/groups/app_data/mod.rs` — `stage_app_data_propose_and_commit`, `consume_proposal_store` integration, lazy-batching behavior.
- `crates/xmtp_mls/src/groups/mod.rs` — `enable_proposals()`, `EnableProposalsOptions` (`force`, `min_version`), two-step bootstrap.
- `crates/xmtp_mls/src/groups/validated_commit.rs` — `ValidatedCommit::from_staged_commit` migrated-branch handling, `validate_one_app_data_update_with_old_value`, steady-state pause via `AppDataUpdate(MinimumSupportedProtocolVersion)`.
- `crates/xmtp_configuration/src/common/mls.rs` — `PROPOSALS_MIN_PROTOCOL_VERSION` constant (`"1.11.0"`).

The `AppDataUpdate` and `AppDataDictionary` APIs are available in OpenMLS behind the `extensions-draft-08` feature flag:

- `MlsGroup::propose_app_data_update()` — Create an AppDataUpdate proposal
- `MlsGroup::app_data_dictionary_updater()` — Build delta updates for processing
- `MlsGroup::process_unverified_message_with_app_data_updates()` — Process commits containing AppDataUpdate proposals
- `AppDataUpdateProposal::update()` / `::remove()` — Construct update/remove operations
- `AppDataDictionary` — BTreeMap-based component storage with ordered, unique entries

## Test Cases

1. **Enable proposals succeeds** — All members support `PROPOSAL_SUPPORT_EXTENSION_ID`. After `enable_proposals()`, the group context contains the extension and subsequent updates use proposal-by-reference.
2. **Enable proposals fails with unsupported member** — One member's key package does not include `PROPOSAL_SUPPORT_EXTENSION_ID`. The group remains in direct-commit mode.
3. **AppDataUpdate accepted when registry policy allows it** — A regular member sends an `AppDataUpdate` targeting `GroupName` (`0x8004`), whose registry entry allows any member. The proposal is accepted and the component value is updated.
4. **AppDataUpdate rejected when policy requires super admin** — A regular member sends an `AppDataUpdate` targeting `SUPER_ADMIN_LIST` (`0x8001`, hardcoded super-admin-only). The proposal is rejected with `InsufficientPermissions`.
5. **AppDataUpdate rejected for immutable component** — A super admin sends an `AppDataUpdate` targeting `ConversationType` (`0xBFFF`) which already has a value. The proposal is rejected because immutable components cannot be modified.
6. **AppDataUpdate rejected for reserved range** — Any member sends an `AppDataUpdate` targeting `0xFF50` (reserved). The proposal is rejected unconditionally.
7. **AppDataUpdate rejected for unregistered id** — A member sends an `AppDataUpdate` targeting an id in the application range (`0xC000-0xFEFF`) with no entry in `COMPONENT_REGISTRY`. The proposal is rejected (deny-by-default).
8. **Pre-commit permission evaluation** — Super-admin Alice proposes an `AppDataUpdate` to `COMPONENT_REGISTRY` (`0x8000`) that relaxes the policy for some application id `X` from super-admin-only to any-member. Regular member Bob proposes an `AppDataUpdate` to `X`. Both proposals are pending; a commit consumes both. The commit MUST be rejected: Bob's write is evaluated against the pre-commit registry (still super-admin-only for `X`), so Bob lacks permission. Alice's registry change in the same commit does not unlock Bob's write — the policy change only takes effect for *subsequent* commits.
9. **Unknown-component tolerance** — A commit contains an `AppDataUpdate(Update)` for an id in `0x8000-0xFEFF` that the receiver has no `Component` impl for. The receiver stores the payload verbatim and does not reject the commit.
10. **Migration preserves state** — A group migrates from GCE to `AppDataDictionary` via the two-step bootstrap. All state (membership, metadata, permissions) is readable from the new ComponentIds and matches the pre-migration values.
11. **Old clients pause on Step A version bump** — Step A bumps `minimum_supported_protocol_version` to `PROPOSALS_MIN_PROTOCOL_VERSION` (`"1.11.0"`). A pre-1.11 client pauses *before* observing Step B's legacy-extension-stripping commit. After upgrading, the client resumes and processes both Step A and Step B.
12. **Steady-state floor bump via AppDataUpdate** — A migrated peer raises the floor by sending an `AppDataUpdate` to `MinimumSupportedProtocolVersion` (`0x800A`). Lower-version peers pause identically to the legacy GCE path.

**Deferred test cases** (re-enable when their feature ships):

- *BatchProposal atomic success / failure* — pair with Section 4.
- *KeyValueDelta Insert fails on existing key* / *Update fails on missing key* — pair with Section 5.

## Open Questions

1. **Migration threshold**: Should the percentage of members required before initiating a version bump be configurable per-group, or should there be a single protocol-wide default (e.g., 100%)?
2. **KeyValue API (Section 5)**: shape and timing for the delta-mutation client API. v1 still exposes a single opaque `app_data` string at `0x8009`.
3. **KeyValueDelta Upsert**: Should `KeyValueDelta` support an `Upsert` operation (insert-or-update) in addition to separate `Insert` and `Update`? Upsert simplifies client code but weakens conflict detection.
4. **Component data size limit**: Should there be a maximum byte size per ComponentId value? Large components could bloat the GroupContext and degrade performance.
5. **Cross-group component allocation**: Section 2.2.2 ships as per-group runtime registration. A global allocator (on-chain or otherwise) is deferred — does the per-group form hold up in practice, or does it need a global namespace before broad application adoption? The 32-byte stable-identifier approach sketched in Section 2.2.2 is one candidate: it lets apps reference components by a 32-byte handle and lets the SDK resolve to the per-group 16-bit ComponentId, so multi-app groups don't need to coordinate on 16-bit slots.
6. **`BatchProposal` (Section 4)**: deferred from v1. Open whether the lazy-batching shape covers the use cases or whether the explicit batch wrapper is still worth shipping.

## Copyright

Copyright and related rights waived via [CC0](https://creativecommons.org/publicdomain/zero/1.0/).

## Implementation Status

Landed on `main`:

- TLS Map / Set component formats (byte-exact serialization).
- `ComponentRegistry` permission architecture at `0x8000` (replaces the original range + override-map split).
- Immutable component handling (counting down from `0xBFFF`).
- AppData handler implementations for all well-known mutable components.
- Two-step bootstrap migration (legacy GCE version bump → proposal-then-commit cutover). Makes pre-d14n migrations safe: lower-version peers pause on the Step A floor bump and never see Step B strip the legacy extensions.
- Proposal, commit, and welcome validation on both pre- and post-migration paths.
- Post-migration intent handlers (Section 3.4) — `MetadataUpdate`, `UpdateAdminList`, `UpdatePermission`, `UpdateGroupMembership`, plus steady-state `MinimumSupportedProtocolVersion` bumps via AppDataUpdate.
- `EnableProposalsOptions { force, min_version }` API surface, exposed through mobile / WASM / node bindings.
- Unknown-component tolerance at the three receive-side sites.
- Versioned `InboxId` TLS wire format on all inbox-id-bearing components.
- `LibXMTPVersion` semver-compliant ordering via the `semver` crate.

Deferred:

- `BatchProposal` (Section 4).
- KeyValue Application Data API and `KeyValueDelta` mutations (Section 5).
- Cross-group ComponentId allocator (the on-chain registry sketched in Section 2.2.2).
- Protocol-version leaf-node extension / server-side `GetGroupVersionSupport` query (Section 3.1) — migration uses the XIP-72 version-floor mechanism without the separate readiness signals.

Optional future work:

- User sub-maps (maps nested inside inbox-id-keyed maps).
- Leaf-node updates for commit-size optimization.
