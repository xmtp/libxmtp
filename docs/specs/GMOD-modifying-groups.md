---
prefix: GMOD
status: draft
---
# Modifying groups

How a group changes after it exists: what a change is made of, who may make one, what a receiving client checks before it applies one, and what the sender does between building a commit and knowing that the group accepted it. Every member validates every commit against state it already holds and against identity state it resolves itself, so a client that accepts a commit the others reject, or rejects one they accept, forks the group.

A change is an MLS commit over proposals. MLS Add, Remove, and Update proposals change the ratchet tree. XMTP group settings and membership records live in the app-data dictionary and change through `AppDataUpdate` proposals. The sender does not trust its own commit: it publishes the commit, reads it back from the group's message topic in the order the backend fixed, and validates it as it would a commit from anyone else. Only then does it apply the commit and send Welcomes to the installations it added.

```mermaid
flowchart LR
  A[App operation] --> B[Build proposals and a commit<br/>on the current epoch]
  B --> P[Publish in one request,<br/>proposals first]
  P --> R[Read back from the<br/>group's message topic]
  R --> V{Validate as any<br/>received commit}
  V -->|passes| M[Apply, then publish Welcomes<br/>anchored at the commit]
  V -->|lost the epoch race| B
  V -->|rejected| F[Fail the operation]
```

## Scope

In scope: the proposal types and senders a client accepts; the membership component and the rule for writing it; how a commit's leaf changes are checked against identity state; how a change is published, read back, and applied; the order and outcome of the checks on a received commit; commit validation against the protocol-version floor; committing another member's proposals; keeping installations current; leaving a group; and group id generation, the group root, and the keys derived from it.

Out of scope: which member may make a given change (`PERM`), component identifiers, encodings, byte limits, and update ordering (`META`), joining and what a Welcome carries (`JOIN`), the association log and exact association state (IDENT-070 and IDENT-071), fork detection, the commit log's use of those keys, and re-add requests (`FORK`), ordered processing and positions (`PROC`), application-message publication (`SEND`), leave-request encoding (CTYPE-014), and consent after rejoin (CONS-025).

| Related | Relation |
| --- | --- |
| `PERM` | Owns whether a proposer may make a change. GMOD-019 says when it is asked; PERM-009 says whom it judges. |
| `JOIN` | Owns Welcome validation and key-package checks. GMOD-009 supplies the sender accounting that JOIN-053 relies on. |
| `META` | Owns all component identifiers and encodings, except the `GroupMembershipEntry` payload below. META-064 owns update ordering; META-010 owns byte limits. |
| `PROC` | PROC-011 and PROC-012 own terminal advancement and holding. This spec supplies the commit-specific checks. |
| `FORK` | Owns re-add requests. GMOD-052 is the validation exception they rely on. |
| `EVENT` | EVENT-001 reports applied membership and metadata changes, removal, and a group pause. |

## Terms

| Term | Meaning |
| --- | --- |
| Proposer | The member whose leaf node signed a proposal. |
| Committer | The member whose leaf node signed a commit. |
| Membership component | The component of the app-data dictionary that records the group's inboxes, stated in section 2. |
| Membership entry | One inbox's value in the membership component: a `GroupMembershipEntry`. |
| Referenced identity state | The association state of an inbox at the `sequence_id` its membership entry names after the commit. Owned by IDENT-070 and IDENT-071. |
| Previous identity state | The association state of an inbox at the `sequence_id` its entry named before the commit, or no state when the inbox was not a member or its entry named 0. |
| Expected additions | For every inbox whose entry the commit adds or changes: the installation keys the referenced identity state associates with it and the previous identity state does not. |
| Expected removals | The installation keys the previous identity state associates with an inbox and the referenced identity state does not, plus every installation key the previous identity state associates with an inbox the commit removes. |
| Stale leaf | A leaf node before the commit whose credential names an inbox that the commit removes from `GROUP_MEMBERSHIP`, or names an inbox whose entry the commit adds, or whose entry's sequence id the commit raises, when that inbox's referenced identity state does not associate the leaf's `signature_key`. |
| Failed installation | An installation key listed in a membership entry's `failed_installations` after the commit. |
| Floor | The minimum client version a group requires: the minimum protocol version component of its dictionary, a semantic version string. |
| Read-back | The sender's receipt of its own commit from the group's message topic, at the sequence id the backend assigned. |
| Terminal rejection | A refusal recorded under PROC-011. A held envelope remains pending under PROC-012. |

## 1. Proposals and commits

MLS defines the proposal types a commit can carry ([RFC 9420 §12.1](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.1)), and the MLS extensions draft adds `AppDataUpdate` ([draft-ietf-mls-extensions-08 §4.7](https://www.ietf.org/archive/id/draft-ietf-mls-extensions-08.html#section-4.7)), which writes one component of the dictionary. XMTP uses four of them. `GroupContextExtensions` is not one: the dictionary is the only group context extension a commit may change, and it changes through `AppDataUpdate`. Pre-shared keys, re-initialisation, external joins, and self-removal are not used. A proposal from anyone who is not a member, an external sender or a new member, is refused.

| Proposal type | Use |
| --- | --- |
| `add` | Adds an installation's leaf node, from its key package (JOIN section 1). |
| `remove` | Removes a leaf node. |
| `update` | Replaces the proposer's own leaf node. |
| `app_data_update` | Writes one component of the dictionary. |

A group is named by its MLS `group_id`, which never changes. TOPIC-001 derives the group's message topic from it. A group's id is the Ed25519 public key of its group signing key. That key is derived from a 32-byte group root, which the creator draws. The root reaches every installation a commit adds inside the Welcome's group context (META section 4). The backend admits a publish to the group's message topic only under a signature by that key (API-230, API-300), so only an installation that holds the root can publish there. A group whose id is 16 bytes is a legacy group, known only from imported archives: its id is random, no installation holds a key for it, and no topic carries it (TOPIC-002).

Each key in the group key table below is derived from the root, one key per use. Each is HKDF-Expand(HKDF-Extract(salt, root), info, 32) under [RFC 5869](https://www.rfc-editor.org/rfc/rfc5869) with SHA-256, where salt is the UTF-8 bytes `xmtp group root`.

The group key table:

| Key | `info` (UTF-8) | Use |
| --- | --- | --- |
| Group signing key | `xmtp group signing key` | Ed25519 private key ([RFC 8032 §5.1.5](https://www.rfc-editor.org/rfc/rfc8032#section-5.1.5)); its public key is the group id; signs `GroupMessageTBS` (API section 3.1) |
| Commit-log signing key | `xmtp commit log signing key` | Ed25519 private key; its public key is the commit-log id; signs `CommitLogEntryTBS` (API section 3.1) |
| Commit-log encryption key | `xmtp commit log encryption key` | ChaCha20-Poly1305 key for commit-log entries (FORK-074) |
| Commit-log nonce key | `xmtp commit log nonce key` | HMAC-SHA256 key that derives commit-log nonces (FORK-074) |

The MLS validation of a proposal and a commit under [RFC 9420 §12.2](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.2) and [§12.4.2](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.4.2) is not restated here; the rules below are what XMTP adds. A commit references proposals that the group's members already hold, so the sender publishes the proposals it created and the commit that references them in one publish request, proposals first: a commit that arrives without its proposals is rejected by every member.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| GMOD-001 | Four proposal types | When a standalone proposal or a proposal inside a commit is not `add`, `update`, `remove`, or `app_data_update`, the client MUST refuse it under the unsupported-proposal table below. | |
| GMOD-002 | Only members propose | When a proposal's or a commit's sender is not a `member` sender ([RFC 9420 §6.1](https://www.rfc-editor.org/rfc/rfc9420.html#section-6.1)) whose leaf node is in the group, the client MUST record a terminal rejection for it. | |
| GMOD-003 | An update keeps its identity | When a standalone `update` proposal's credential names an inbox other than the proposer's, the client MUST record a terminal rejection for it. | A member could otherwise place its leaf under another member's inbox. |
| GMOD-004 | Proposals travel with their commit | When a client publishes a commit that references proposals it created, it MUST publish those proposals and the commit in one publish request, the proposals first in the order it created them. | |
| GMOD-040 | The group id is a public key | When a client creates a group of any conversation type, other than a restored placeholder under ARCH-015, it MUST draw a 32-byte group root from a cryptographically secure random source for that group alone. It MUST set the MLS `group_id` to the 32-byte public key of the group signing key derived from that root under GMOD-041. | A root that is predictable or shared gives the publish key of a group to an installation the group never added. |
| GMOD-041 | Keys derive from the root | The client MUST derive each key in the group key table from the group root with the salt and `info` the table gives, and MUST use each derived key only for the use the table names. | Installations that derive differently compute different topics and signatures, and a key used for two purposes lets a signature for one be presented as the other. |
| GMOD-042 | The root stays in the group | A client MUST NOT write a group root, a private or symmetric key in the group key table, or a `GroupInfo` whose group context holds a root, to any destination other than its own client database and a Welcome or welcome pointee for an installation that a commit adds. | Whoever obtains the root can publish to the group's topics and find its commit-log topic for as long as the group exists, because the root never changes. |

The unsupported-proposal table gives the outcome after MLS validation has accepted the message. Earlier MLS failures follow PROC-011 and PROC-012. A held proposal is not retained as an accepted proposal for a later commit.

| Unsupported proposal | Standalone outcome | Commit outcome |
| --- | --- | --- |
| `GroupContextExtensions` | Hold | Hold |
| `PreSharedKey` | Hold | Terminal rejection |
| `ReInit` | Hold | Hold |
| `ExternalInit` | Hold | Hold |
| `SelfRemove` | Hold | Hold |
| `AppEphemeral` | Hold | Hold |
| Custom proposal | Hold | Hold |

### 1.1 Proposal-list validity

The proposal-list checks of [draft-ietf-mls-extensions-08 §4.7](https://www.ietf.org/archive/id/draft-ietf-mls-extensions-08.html#section-4.7) apply without a `GroupContextExtensions` proposal. After these checks, the client uses META-064 for both validation and application of app-data updates.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| GMOD-030 | Reject conflicting component operations | When a commit carries `AppDataUpdate` proposals, the client MUST reject an invalid proposal list under draft-ietf-mls-extensions-08 §4.7, including an Update and a Remove, or more than one Remove, for the same component id, whether or not the commit contains a `GroupContextExtensions` proposal. | Conflicting component operations can make clients compute different state for one epoch. |

### 1.2 Selecting pending membership proposals

An accepted Add can outlive the commit that first referenced it. Acceptance of
that proposal does not prove that its installation or key package is still
eligible for a later commit. The sender resolves the intended membership and
fetches current key packages before selecting pending membership proposals.
It compares complete key packages, not only installation signature keys. A
fetch failure is not evidence that an accepted proposal is obsolete.

The sender reuses a current eligible proposal with its original proposer. A
replacement Add uses the sender's own authority. If it cannot replace an
obsolete Add, it can omit the Add and the whole signed membership update that
depends on it. It then recomputes the dependent Adds and Removes. It does not
edit another proposer's signed update. An empty commit can advance the epoch
when only obsolete work remains. Omitted proposals then expire under the MLS
epoch rules; their authors can propose them again.

The sender also selects compatible whole membership updates before applying
their dictionary changes. For example, two proposals can each insert the same
new inbox, but a commit cannot apply both inserts. Every commit builder uses
the selected dictionary changes and retains Welcomes for the selected Adds.
This includes metadata, permission, and automatic key-update commits.

These are sender selection rules. Receivers continue to validate the published
commit against its stated identity references. They do not fetch the latest
key package to decide whether to accept a commit.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| GMOD-038 | Select one consistent proposal set | When preparing a commit from pending membership proposals, the sender MUST select a set that satisfies the resolved membership, key-package validation, and proposer authority rules. It MUST compute dictionary updates and the MLS commit from that same set, MUST preserve accepted proposal references until normal commit resolution, and MUST NOT report an omitted app-requested change as successful. | An obsolete Add can block publication; inconsistent selection can make members disagree. |

### 1.3 Omitting uncommittable pending proposals

A receiver checks a standalone proposal against the committed group state alone, so every member accepts or rejects it alike. Proposals accepted one at a time can still form a commit that every member rejects: two inserts of one key, an Update and a Remove of one component, two super-admin removals that leave none, or a Delete that the membership after the commit forbids.

The sender of a commit of pending proposals therefore applies the receivers' commit validation first. It walks the pending proposals in order and keeps each one that receivers would accept after the ones kept before it. It judges membership-dependent rules with the membership the kept proposals leave, and repeats the walk until that membership stops changing. It does not look ahead, so a later proposal that would make an earlier one valid may itself be omitted. Omitted proposals expire under the MLS epoch rules, as in section 1.2. No client proposes a leaf `Update`, and receivers check only its credential (GMOD-003), so a commit never carries one.

The sender's own requested change is not filtered. An author publishes its proposals with the commit that carries them in one request (API-220), so when another member's commit omits them, the author's commit goes stale and is rebuilt under GMOD-036.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| GMOD-047 | Omit uncommittable pending proposals | When preparing a commit from pending proposals, the sender MUST count each proposal reference once, MUST omit each pending proposal that receivers' validation of the commit would reject after the proposals kept before it, judged with the membership the kept proposals leave, MUST omit a membership delta that adds an inbox with no Add of that inbox, or removes an inbox that a leaf node's credential names before the commit with no Remove of that inbox, among the carried proposals, and MUST NOT carry a leaf `Update` proposal. When the intent's own requested change conflicts with the kept proposals, the sender MUST fail that intent and MUST NOT omit a kept proposal because of the conflict. | One member's accepted proposal would otherwise fail every commit of pending proposals, so no member could send or change the group. |
| GMOD-045 | Store a repeated proposal once | When a client accepts a standalone proposal whose reference is already pending, it MUST NOT store a second copy. | A commit covers a reference once. A second copy counts an update twice, and removing one copy deletes the stored proposal both share. |

## 2. The membership component

The membership component names every inbox that is a member and, for each, the identity state its installations are checked against and the installations the group could not add. It is what a joiner validates a Welcome's ratchet tree against (JOIN section 8) and what every member validates a commit's leaf changes against (section 3). The component is a map from inbox id to an encoded `GroupMembershipEntry`; META-010 and META-011 own the map and inbox-id encodings, and META section 2 assigns the component id.

```proto
message GroupMembershipEntry {
  message V1 {
    uint64 sequence_id = 1;
    repeated bytes failed_installations = 2;
  }

  oneof version {
    V1 v1 = 1;
  }
}
```

A `sequence_id` is a promise the sender makes: it resolved that inbox's identity updates through that sequence id and built the commit from the state it found. It is 0 only for the creator's own entry at creation, before any identity reference is needed. Every sequence id a commit writes is less than the sequence id the commit itself receives, because the identity update was published first and the backend assigns sequence ids in publication order, so a reference at or after the commit can never resolve.

JOIN-093 owns partial success when some key packages are invalid. GMOD-009 accounts for the installations in each inbox, including an installation for which the backend returns no key package. JOIN section 8 owns validation of the membership asserted by a Welcome.

A failed installation is one the group holds no leaf for. A membership change can be built when an installation's key package cannot be fetched and published after that installation has a leaf: a later fetch succeeds and the commit adds it, or another member's commit added it first. The installation is then not failed. A listed key never exempts a leaf from the removal check of section 3 (GMOD-046), but under GMOD-052 it lets a super admin remove that leaf without a re-add.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| GMOD-005 | Membership entry format | The client MUST encode each member's value in `GROUP_MEMBERSHIP` as the `GroupMembershipEntry` defined above, with `v1` set and exactly one entry per member inbox. | |
| GMOD-006 | An added inbox references resolved state | When a commit adds an inbox, the sender MUST set its entry's `sequence_id` to the sequence id of the latest identity update it resolved for that inbox, greater than 0. | |
| GMOD-007 | References precede the commit | If any entry's `sequence_id` is not less than the commit's own sequence id, or an entry the commit adds has a `sequence_id` of 0, then the client MUST record a terminal rejection for the commit. | A reference at or after the commit can never be resolved, and 0 asserts nothing. |
| GMOD-008 | Sequence ids never decrease | If an inbox's entry after the commit has a `sequence_id` less than before it, then the client MUST record a terminal rejection for the commit. | A lowered reference would let a revoked installation back in. |
| GMOD-009 | Every installation is accounted for | When a sender publishes a commit that adds an inbox or changes its `sequence_id`, the sender MUST ensure that every installation key in that inbox's referenced identity state is either the `signature_key` of a leaf after the commit or in that inbox's `failed_installations`. | An omitted installation loses access, and a later joiner rejects the incomplete membership. |
| GMOD-044 | A failed installation has no leaf | When a client publishes a commit for its own membership update, which adds, removes, or refreshes inboxes, it MUST NOT list in any membership entry that update writes the `signature_key` of a leaf that the ratchet tree holds after the commit. | A listed key that has a leaf lets a super admin's commit remove that leaf under GMOD-052 without a re-add or an identity change. |
| GMOD-049 | A failure belongs to its inbox | If a commit leaves an inbox's entry listing in `failed_installations` a key that entry did not list before the commit, and that key is neither associated with the inbox by its referenced identity state nor the `signature_key` of a leaf node before the commit whose credential names the inbox, then the client MUST record a terminal rejection for the commit. | A failure filed under another inbox does not account for that inbox's missing installation under JOIN-053, so joiners reject a Welcome for a membership that members accepted. |

## 3. Leaf changes against identity

A commit's Add and Remove proposals are not taken at their word. The membership component says which identity state each inbox is at, and the difference between the previous and the referenced identity state says which installations may be added and which must be removed. A leaf added outside the expected additions is a leaf under an identity that never authorised it; a removal missing from the expected removals leaves a revoked installation reading the group. The expected additions are one set across all inboxes, so each new leaf must also name the inbox that owns its key. This includes the committer's leaf in the update path, because MLS lets that leaf carry a new credential; that leaf must also keep the committer's inbox (GMOD-043). A key that has no leaf before the commit is exempt from the removal check, because the commit has nothing to remove; a key listed as failed is not exempt when it has a leaf.

The difference between two identity states misses two kinds of leaf. A leaf that an earlier commit kept past its revocation is in neither state, and an entry raised from 0 has no previous state to compare with. A commit that raises or removes an inbox therefore also removes that inbox's stale leaves, so the inbox's next identity update repairs a leaf left behind.

The checks read identity state the client resolves itself. When it does not hold an inbox's association state at the `sequence_id` an entry names, it fetches that inbox's identity updates through that sequence id before it decides, so that two members with different caches reach the same answer. A reference the backend holds nothing for can never resolve and is rejected. The creator's unchanged zero-sequence entry is a creation placeholder: only a key and inbox already paired in an authenticated committed leaf can use it, without resolving sequence 0. A new key still needs a nonzero identity reference.

One exception exists for recovery. A super admin, or either member of a DM, may remove and re-add an installation in one commit, replacing its leaf without an identity change. A super admin may also remove a leaf whose installation is listed as failed. `FORK` owns when that is done; this section owns that every member accepts it.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| GMOD-010 | Only expected leaves are added | If a commit adds a leaf node whose `signature_key` is not in the expected additions and is not the `signature_key` of a leaf node already in the group, then the client MUST record a terminal rejection for the commit. | This is the check that stops a member placing a leaf under an identity that never authorised it. |
| GMOD-046 | Expected leaves are removed | If, after the GMOD-052 adjustments, the set of `signature_key`s of the leaf nodes a commit removes is not equal to the set of `signature_key`s of leaf nodes before the commit that are expected removals or stale leaves, then the client MUST record a terminal rejection for the commit. | A removal the identity state does not call for evicts a member. A revoked installation's leaf that a commit keeps, because the commit omits its Remove or an earlier commit left the leaf behind, keeps reading the group. |
| GMOD-012 | Signing keys belong to their inboxes | When the client validates a commit, it MUST record a terminal rejection for the commit unless, for the committer's leaf, the leaf in the commit's update path, and the leaf of every `add` and `update` proposal, the leaf's `signature_key` belongs to the credential's inbox in its referenced identity state, or that inbox's entry is 0 both before and after the commit and the same key and inbox pair exists in a committed leaf. | GMOD-010 compares additions with the expected additions of all inboxes together, and MLS lets an update path change the leaf's credential. Without this check, a member could place a leaf it signs under another member's inbox and then send messages and proposals as that member. |
| GMOD-043 | An update path keeps its identity | When the leaf in a commit's update path names an inbox other than the committer's, the client MUST record a terminal rejection for the commit. | The update path replaces the committer's own leaf, as an `update` proposal does under GMOD-003. A key that two inboxes both hold would otherwise let its holder move its leaf from one inbox to the other. |
| GMOD-013 | Resolve before deciding | When validation needs an inbox's association state at a nonzero `sequence_id`, the client MUST resolve that exact state under IDENT-070 and IDENT-071 before it decides on the commit. | Deciding on cached state makes members disagree about the same commit. |
| GMOD-014 | Unresolvable references are terminal | When a client has fetched an inbox's identity updates and the backend holds no update at the `sequence_id` an entry names, the client MUST record a terminal rejection for the commit. | A reference to a state never published can never become valid. |
| GMOD-052 | Permitted readders may re-add | When the committer is a super admin, or the group is a DM whose `DM_MEMBERS` names the committer's inbox, the client MUST remove keys that the commit both adds and removes from its actual addition and removal sets before the GMOD-010 and GMOD-046 comparisons. When the committer is a super admin, the client MUST then remove keys present in both the remaining actual removals and failed installations from both of those sets. | Without the DM case, every member rejects the readd commit that FORK-062 asks of a DM's permitted readder, so a forked DM member is never repaired. The failed-key case stays with super admins: in a DM it would let one member remove the other's installation with no Welcome. |

## 4. Publishing a change

An app's operation becomes an intent the client holds until the group accepts it. The client builds proposals and a commit on the epoch it holds, publishes them, and then waits for the read-back. The commit is not applied from the sender's own copy: the sender processes the group's message topic in order like every other member, meets its own commit there, and validates it under section 5. A commit that fails validation on the sender fails on every member, and the operation is reported to the app as failed.

Two commits built on the same epoch race, and the backend's order decides the winner. The loser reads its own commit back after the group's epoch has moved past the epoch it was built on. That commit is stale: applying it would fork the sender from the members that applied the winning commit, so the sender discards it and builds the change again on the current state. SEND-014 owns when that recovery applies to an intent and when a rejection ends it instead.

A commit that adds installations owes them a Welcome. The Welcome names the commit by its sequence id on the group's message topic (JOIN section 6), so it cannot be built until the read-back, and an installation that never receives it has no way to join. Welcomes are published after the commit is applied, and the operation is not complete until the backend has accepted every one of them.

A commit may reference proposals other members published. Every member keeps a received proposal only after validating it, so a proposal that one member rejects is one every member rejects, and a commit that references it fails everywhere the same way. PERM-009 judges each proposal by its proposer, so the committer needs no authority of its own over the changes it commits.

A standalone proposal is judged against the committed membership, because the commit that will reference it is not yet known. A delete that is authorized only because its key leaves the membership in the same commit, such as a removed member's profile entry under PERM-027, is therefore an inline proposal of the member-removal commit and is validated there under section 5.

The sender of a member removal cleans up after the inboxes it removes, so a departed member's name or admin role does not outlive its membership. Cleanup is a courtesy of the sender, not a condition of the commit: a remover whose policies forbid a cleanup delete still removes the member, and a receiver accepts a removal with or without it.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| GMOD-016 | Apply only what the topic returns | A client MUST NOT apply a commit it built before it has read that commit back from the group's message topic at the sequence id the backend assigned, and MUST NOT exempt it from any check of section 5 because it built it. | A sender that applies first and loses the race, or built a commit the others reject, is forked from the group. |
| GMOD-036 | A stale commit is rebuilt | If a client reads back a commit it published and the group's current `GroupContext.epoch` is greater than the epoch the commit was built on, then the client MUST NOT apply that commit and MUST build the change again on the current group state before it publishes again. | Applying a commit from a past epoch forks the sender from every member that applied the winning commit. |
| GMOD-018 | Welcomes follow the commit | When a client has applied a commit it built that adds leaf nodes, it MUST publish a Welcome to every installation the commit added, with `message_cursor` equal to the commit's sequence id, and MUST NOT publish any Welcome for that commit before the read-back. | An installation that receives no Welcome cannot join the group. |
| GMOD-019 | Received proposals are validated first | When the client receives a standalone proposal, it MUST check its type and sender under section 1, the same-inbox condition for Update, the Add or Remove authority under PERM-015, and the component structure and write authority under META and PERM for `AppDataUpdate`, before retaining it as an accepted proposal. | A proposal accepted by only some members can make a later commit fork the group. |
| GMOD-039 | Clean up removed inboxes | When a client builds a commit that removes an inbox from `GROUP_MEMBERSHIP`, it MUST include in that commit an inline delete of the inbox's key from each other registered inbox-keyed map and from `ADMIN_LIST` where present, for each delete the committed policies authorize with the context of PERM-027, and MUST omit each other delete. A receiver MUST NOT reject a removal commit for omitting a cleanup delete. | A stale name or role would outlive the membership, but a forbidden cleanup must not block the removal. |
| GMOD-050 | Carry identity leaf changes inline | When a client builds a commit for its own membership update, it MUST carry each Add and each Remove that update makes for a leaf whose inbox is in `GROUP_MEMBERSHIP` both before and after the commit as an inline proposal of that commit. | Receivers judge a standalone Add or Remove under PERM-003 and PERM-015 as a change to the inbox. They reject one for a super admin's revoked installation, from a member without that policy, or in a DM, so the other members cannot stage the sender's commit and the group forks. |

## 5. Receiving a commit

A received commit passes through the checks in this spec and in `PERM`, and is applied only when every one passes. A commit that fails leaves the group's state and epoch unchanged. What happens next depends on why it failed. PROC-011 owns terminal rejection and advancement. PROC-012 owns holding after a storage failure, an unresolved identity dependency, or an unsupported proposal. The proposal table in section 1 distinguishes an unsupported standalone PSK from a terminal committed PSK.

The floor is different from both. A client below the group's floor cannot be sure it can even parse what the group now carries, and a rejection it issues alone is a fork. So it pauses: it applies nothing further on that group, publishes nothing to it, and records no rejection, until its version reaches the floor and it reprocesses from where it stopped. While the committed floor is supported, a commit that raises it is validated before it can pause the client. A terminal validation failure rejects that commit; an unresolved dependency or storage failure holds it under PROC-012. A committed floor that is already unsupported pauses the client before further commit validation. Versions compare under [Semantic Versioning 2.0.0 §11](https://semver.org/spec/v2.0.0.html#spec-item-11), including its pre-release precedence, unlike the backend's minimum under CONF-050. The floor only rises, because a lowered floor lets a paused client apply commits it cannot interpret. A new group is created with a floor at the first version that supports the dictionary, and PERM-023 says when a release raises it.

A commit the client applies is recorded for the app as a membership-change message whose payload is a `GroupUpdated` transcript (CTYPE-014, META-074). The transcript is derived from the validated commit: who committed it, which inboxes it added or removed, and which settings it changed. It is a record the client writes for itself, never content a member sends, so an application message that carries a transcript type is not a transcript. GMOD-054 says when such a message is not published.

META-010 owns component encodings and byte limits. Standalone checks do not resolve a future commit's identity references, compare its aggregate leaf changes, or apply its proposed floor. The client repeats authorization at commit time under PERM-009 and checks the full change here.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| GMOD-021 | Validate, then apply | The client MUST apply a commit only after the checks in GMOD, PERM, and META have passed, with app-data validation and application in the order META-064 defines. When any check fails, it MUST leave the group state and epoch unchanged. | |
| GMOD-022 | Judge before the floor pauses | When the committed floor is not greater than the client's version and a commit would raise it above that version, the client MUST complete the other commit validation checks before pausing for that raise, and MUST terminally reject a terminal validation failure under PROC-011. | An unauthorized floor raise would otherwise pause the group. |
| GMOD-025 | Pause below the floor | While the committed floor exceeds the client's version, or a commit that passes all other validation would raise it above that version, the client MUST hold before that commit under PROC-012 and MUST NOT publish a message, proposal, or commit in the group. When its version reaches the floor, it MUST resume from the first held envelope. | Rejecting or skipping a commit that newer members accept forks the group. |
| GMOD-026 | The floor only rises | If a proposal sets the floor to a version whose precedence under GMOD-027 is lower than the committed floor's, or removes the floor while one is committed, then the client MUST record a terminal rejection for it. | Clients that order two floors differently reject different proposals, and a commit that carries one forks the group. |
| GMOD-027 | Version precedence | When the client compares its version with a floor, it MUST use [Semantic Versioning 2.0.0 §11](https://semver.org/spec/v2.0.0.html#spec-item-11), including pre-release precedence; a malformed committed floor MUST NOT cause a pause. | |
| GMOD-034 | Transcripts come from validated commits | When the client applies a commit, it MUST derive the membership-change message it stores for that commit from the validated commit, with the committer and the added, removed, and changed entries taken from the commit's actor and its effect on the pre-commit state. | A transcript written from anything but the commit shows the app membership changes that did not happen. |
| GMOD-054 | Transcripts are never published at any depth | When an application message's content, or content nested in a reply at any depth, compressed or not, has type `xmtp.org/group_updated` or `xmtp.org/group_membership_change`, the client MUST NOT publish that message. When a prepared attempt for such a message is already saved under SEND-006, the client MAY publish its saved bytes under SEND-007. | A member could send a forged transcript that other apps show as a membership change, or a reply that every receiver reports as a decode failure under GMOD-053. The saved bytes of an attempt may already be on the backend, and SEND-007 does not let the client replace them. |
| GMOD-053 | Application transcripts are not decoded | When the client decodes an application message whose content, or content nested in a reply at any depth, has type `xmtp.org/group_updated` or `xmtp.org/group_membership_change`, it MUST report a decode failure that preserves the message under CTYPE-008, and MUST NOT decode that content's payload as a transcript. | A member could send a transcript that apps show as a membership change that never happened, carrying lists large enough to exhaust the recipient's memory. |

## 6. Keeping membership current

An inbox's installations can change after it joins: a device is added or revoked through an identity update (IDENT-070), and the group's leaf nodes no longer match. Any member repairs this. A client that syncs a group fetches the identity updates of every member on a schedule, and when an inbox's latest sequence id is greater than its entry, publishes a commit that raises the entry and makes the expected additions and removals. The revoked installation loses the group at that commit, and the new one receives a Welcome. Which member does it first does not matter: the commits race under section 4 and the loser finds nothing left to do.

A refresh is built from the membership at one moment and published later, after other commits can have landed. Its entries raise inboxes that were members when it was built. When one of those inboxes has since been removed, the refresh leaves it out, and when a later commit raised an entry past the refresh, the entry keeps the higher sequence id. Only an app's request to add an inbox inserts one.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| GMOD-029 | Members repair changed identity state | When a membership refresh for an active group resolves an inbox's identity state at a sequence id greater than its membership entry's, the client MUST publish a membership change that raises the entry to that sequence id and makes the expected additions and removals under sections 2 and 3. | A new device cannot add itself, and a revoked one keeps reading until a member removes it. |
| GMOD-051 | A refresh never inserts an inbox | When a client publishes a membership change that records which of its entries are refreshes under GMOD-029, it MUST NOT insert into `GROUP_MEMBERSHIP` an inbox whose entry the change records as a refresh. | A refresh built before a commit removed an inbox would undo that removal. |

## 7. Leaving a group

A leave request is an authenticated application message in the group. CTYPE-014 owns its content type and `LeaveRequest` encoding. Its sender asks for removal of the sender's inbox; the note carries no authority over another inbox. Sending the request does not publish a self-removal commit. The requesting inbox is pending removal until another member commits the removal.

PERM-025 owns authority to act on a leave request. The removal still passes PERM-003 and PERM-015 and the membership checks here. A request can remain pending while no authorized super-admin client can publish the removal. CONS-025 owns consent if the inbox later rejoins.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| GMOD-031 | Request a permitted leave | When an app requests leave, the client MUST reject the operation if its inbox is not a member, the group has only one member inbox, the conversation is a DM, or its inbox is a super admin. Otherwise, if its inbox has no pending leave request, it MUST send a `LeaveRequest` under CTYPE-014 without publishing a self-removal commit. | |
| GMOD-032 | Authenticate the leaving inbox | When the client processes a leave request, it MUST take the inbox to remove from the authenticated MLS application-message sender and MUST NOT take it from the request payload. For a request from its own inbox, it MUST expose the group as pending removal until a commit removes that inbox. | A payload chosen by one member cannot authorize another member's leave. |
| GMOD-033 | Complete an authorized leave | When a client processes pending leave requests and has authority under PERM-025, it MUST publish a removal commit for the requesting inboxes that are still members, subject to PERM-003 and PERM-015. It MUST report removal complete only after it applies a commit that removes the inbox, and MUST keep a request pending when publication or validation fails. | Publication of a request alone does not remove any installation from the ratchet tree. |

## Known limitations

The receiver tolerates a commit that adds an inbox without adding every installation the referenced identity state associates and without listing the rest as failed. GMOD-009 binds the sender; existing members accept the commit, and only a later joiner rejects the Welcome under JOIN-053.

A membership refresh does not retry a failed installation key that remains associated in both identity states. Its key package becoming valid does not by itself add it to the group.

Receivers do not reject a membership entry that keeps listing a key that has a leaf, and older clients wrote such entries. The listing does not exempt the leaf from removal under GMOD-046, but under GMOD-052 a super admin's commit can remove that leaf without a re-add. A leaf that a commit accepted before GMOD-046 kept past its revocation stays until a commit raises its inbox's entry or removes the inbox, and that commit must remove it as a stale leaf. If the inbox publishes no later identity update and stays a member, the leaf stays.

GMOD-044 covers a client's own membership updates. A commit that keeps another member's accepted membership delta keeps it whole or omits it whole (GMOD-038), and a readd or a standalone membership proposal carries the failed lists already written, so these can keep a key with a leaf that an earlier writer listed. They do not add a leaf for a listed key: selection omits the Add of a key that a kept delta lists as failed, and a pending proposal never outlives a commit. Omitting such a delta would hold back its other changes until its author proposes again, while a revocation that a conforming client commits removes the leaf either way.

A membership change stored before its refresh entries were recorded apart from its requested adds treats every entry as an add. When it publishes, it can insert an inbox that a commit removed after the change was built. An older build that publishes a stored change after a downgrade ignores that record in the same way, as it does for the changes it builds itself. The record is not versioned to make older builds fail closed: an older build cannot decode a newer payload version, and an own change it cannot decode stops every later publication in its group.

MLS discards proposals that a commit did not reference once the epoch advances. A member whose proposal was omitted from an accepted commit has to propose again if the change is still needed.

The floor pauses only the clients below it. Members at or above it continue, and the paused client's unpublished changes are built on an epoch the group has left and are rebuilt under SEND-014 after it resumes.

A membership refresh is triggered by client activity. Its interval is an implementation choice, not a deadline for removing a revoked installation or adding a new one. No bound applies while all members are offline.

Clients that predate GMOD-043, and the coverage of `add` and update path leaves in GMOD-012, apply a commit with an unbound leaf that newer clients reject. Without a version floor (PERM-023), the group forks between them. A forged commit causes this, and so can an older client's own refresh, because it adds a key package whose credential names another inbox, which JOIN-093 excludes.

Receivers check only that a readd adds back the installation it removes, not that the added key package is the newest one that installation published. A super admin, or either member of a DM (GMOD-052), can readd an installation with a key package whose private keys it destroyed under JOIN-074; that installation processes the removal, cannot open the Welcome, and keeps a leaf it cannot use, so it is out of the conversation until it is readded again. A super admin can already remove any member; for a DM member this is a new power.
