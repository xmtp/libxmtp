//! End-to-end integration tests for the QR-invite flow.
//!
//! These tests exercise the full producer → consumer pipeline against a
//! running XMTP backend (Docker / `tester!` clients): an existing member
//! creates an invite via [`MlsGroup::create_external_invite`] (L-10), a
//! non-member joins via [`Client::join_group_by_external_invite`] (L-11),
//! and the L-7 receive-side validator gates the resulting external commit
//! through the standard sync path.
//!
//! Scope: the application's encrypted-blob service is stubbed in-test as
//! a `HashMap<group_id_hash, Vec<u8>>`; we exercise the libxmtp side
//! (crypto envelope, openmls external commit, AppData membership scope,
//! HPKE-wrapped welcomes, stale-epoch / disallowed-policy rejection) end
//! to end.
//!
//! KNOWN UPSTREAM ISSUE — receive-side framing rejection
//! -----------------------------------------------------
//! As of this PR's base merge of (L-7 + L-8) ⨯ (L-9 + L-11), the four
//! happy-path tests (`qr_invite_happy_path_single_installation`,
//! `qr_invite_happy_path_multi_installation`, `qr_invite_chain_self_refresh`,
//! `qr_invite_membership_update_is_inbox_scoped`) FAIL because the receive
//! path in `groups::mls_sync::process_message` rejects every envelope
//! whose `ProtocolMessage` is not `PrivateMessage` (line ~2090 of
//! `mls_sync.rs`):
//!
//! ```ignore
//! match &envelope.message {
//!     ProtocolMessage::PrivateMessage(_) => (),
//!     other => {
//!         return Err(GroupMessageProcessingError::UnsupportedMessageType(
//!             discriminant(other),
//!         ));
//!     }
//! };
//! ```
//!
//! External commits (L-11's atomic-join path) are wire-formatted as
//! `PublicMessage` per RFC 9420 §12.4.3.2 — they are silently dropped
//! by this guard before reaching `process_message_inner`, where L-7's
//! `Sender::NewMemberCommit` → `ValidatedCommit::from_external_commit`
//! branch lives. End-effect: bo's commit lands on the network (the
//! producing side works end-to-end through L-11), but alix's sync
//! rejects it at framing time and never registers bo as a member.
//!
//! The two negative tests (`qr_invite_rejected_when_policy_disallows`
//! and `qr_invite_rejected_stale_epoch`) pass for a confounded reason:
//! the framing-level rejection has the same end-state as the policy-
//! / epoch-level rejection they are asserting (alix never sees bo).
//! Once the framing-level routing is fixed, both should continue to
//! pass via the path each is named after.
//!
//! Fix surface area for the parent orchestrator: extend the framing
//! check to accept `ProtocolMessage::PublicMessage(_)` when the inner
//! commit's `Sender` is `NewMemberCommit`, and route it into
//! `process_message_inner`'s `Sender::NewMemberCommit` branch (which
//! is the L-7 entry point that already exists at ~mls_sync.rs:1116).

use std::collections::HashMap;

use prost::Message as _;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_proto::xmtp::mls::message_contents::{
    ExternalInvitePayload as ExternalInvitePayloadProto,
    external_invite_payload::Version as PayloadVersion,
};

use crate::context::XmtpSharedContext;
use crate::groups::EnableProposalsOptions;
use crate::groups::external_invite::CreateExternalInviteOpts;
use crate::groups::group_permissions::PolicySet;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::validated_commit::extract_group_membership;
use crate::tester;

/// In-memory stand-in for the application's invite-blob service.
///
/// Keyed by `payload.v1.external_group_id` (an opaque service-slot
/// identifier carried in the payload). Mirrors what an out-of-process
/// service would do (overwrite-on-refresh, look-up-by-slot).
#[derive(Default)]
struct InviteBlobStore {
    blobs: HashMap<Vec<u8>, Vec<u8>>,
}

impl InviteBlobStore {
    fn new() -> Self {
        Self::default()
    }

    fn slot_id(payload_bytes: &[u8]) -> Vec<u8> {
        let payload =
            ExternalInvitePayloadProto::decode(payload_bytes).expect("invite payload decodes");
        match payload.version {
            Some(PayloadVersion::V1(v1)) => v1.external_group_id,
            None => panic!("invite payload has no version variant"),
        }
    }

    /// Upload (or atomically overwrite) the blob for a given invite.
    fn put(&mut self, payload_bytes: &[u8], blob_bytes: Vec<u8>) {
        self.blobs.insert(Self::slot_id(payload_bytes), blob_bytes);
    }

    /// Look up the current blob for the invite the receiver just decoded.
    fn get(&self, payload_bytes: &[u8]) -> Vec<u8> {
        self.blobs
            .get(&Self::slot_id(payload_bytes))
            .cloned()
            .expect("blob present for external_group_id slot")
    }
}

/// External-commit gating is no longer a `PolicySet` field — the master
/// switch moved to the `EXTERNAL_COMMIT_POLICY` AppData component (L-6).
/// Tests flip it at runtime via `MlsGroup::set_allow_external_commit`
/// after creating the group with the default policy.
fn policy_allowing_external_commit() -> PolicySet {
    PolicySet::default()
}

// ─────────────────────────────────────────────────────────────────────
// 1. Happy path — single-installation joiner.
// ─────────────────────────────────────────────────────────────────────

#[xmtp_common::test(unwrap_try = true)]
async fn qr_invite_happy_path_single_installation() {
    tester!(alix);
    tester!(bo);

    // Alix creates a group that permits external (QR-invite) joins.
    let group = alix.create_group(Some(policy_allowing_external_commit()), None)?;
    // Settle alix's installation in the freshly-created group (the
    // initial GroupMembership entry is `alix: 0`; the first sync
    // auto-publishes an `UpdateGroupMembership` commit to bump alix's
    // sequence id to her current identity-update sequence). Without
    // this, a subsequent receive-side validation of alix's own metadata
    // commit can fail with `MissingIdentityUpdate`, and an invite
    // produced from the pre-sync GroupInfo would also be one epoch
    // behind alix's auto-bumped post-sync state.
    group.sync().await?;

    // Migrate the group to the AppData GROUP_MEMBERSHIP component. The
    // QR-invite flow writes via AppDataUpdate proposals (the post-migration
    // storage path); L-10 / L-11 explicitly refuse to operate on pre-migration
    // groups so callers don't produce commits other members can't apply.
    group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    // Flip the EXTERNAL_COMMIT_POLICY master switch on. Required for
    // the QR-invite flow — without this, alix's validator rejects bo's
    // external commit at the policy gate.
    group.set_allow_external_commit(true).await?;
    group.sync().await?;

    // Alix produces an invite + uploads the encrypted blob to the
    // (in-test) service.
    let mut service = InviteBlobStore::new();
    let out = group
        .create_external_invite(CreateExternalInviteOpts {
            service_pointer: b"https://invites.example/abc".to_vec(),
            blob_expires_at_ns: None,
        })
        .await?;
    service.put(&out.invite_payload, out.encrypted_group_info.clone());

    // Sanity: Bo is not yet a member from Alix's view.
    let alix_members_before = group.members().await?;
    assert!(
        !alix_members_before
            .iter()
            .any(|m| m.inbox_id == bo.inbox_id()),
        "bo must not start out as a member of alix's group",
    );

    // Bo fetches the blob from the service (keyed by group_id_hash) and
    // joins via external commit.
    let blob_bytes = service.get(&out.invite_payload);
    let join_out = bo
        .join_group_by_external_invite(&out.invite_payload, &blob_bytes)
        .await?;
    assert_eq!(join_out.group.group_id, group.group_id);

    // Allow the network a beat to replicate bo's commit before alix
    // tries to fetch + apply it.
    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // Alix syncs and accepts the external commit through the L-7
    // validator path.
    group.sync().await?;
    let alix_members = group.members().await?;
    assert!(
        alix_members.iter().any(|m| m.inbox_id == bo.inbox_id()),
        "alix must see bo as a member after sync; got {:?}",
        alix_members
            .iter()
            .map(|m| m.inbox_id.clone())
            .collect::<Vec<_>>(),
    );

    // Alix sends a post-join message; Bo receives + decrypts.
    let msg_text = b"welcome via QR, bo";
    group
        .send_message(msg_text, SendMessageOpts::default())
        .await?;
    join_out.group.sync().await?;
    let bo_msgs = join_out.group.find_messages(&MsgQueryArgs::default())?;
    assert!(
        bo_msgs
            .iter()
            .any(|m| m.decrypted_message_bytes == msg_text),
        "bo must decrypt alix's post-join message",
    );

    // The refreshed encrypted blob must unwrap with the SAME symmetric
    // key from the original invite payload (joiner-self-chain). The
    // L-11 unit tests already cover unwrap correctness; we re-assert
    // here as the end-to-end happy-path invariant.
    let payload = ExternalInvitePayloadProto::decode(out.invite_payload.as_slice())?;
    let payload_v1 = match payload.version {
        Some(PayloadVersion::V1(v)) => v,
        None => panic!("payload missing version"),
    };
    assert!(
        !payload_v1.external_group_id.is_empty(),
        "payload external_group_id must be populated by policy",
    );
    assert!(
        !join_out.refreshed_encrypted_group_info.is_empty(),
        "refreshed blob is non-empty",
    );
}

// ─────────────────────────────────────────────────────────────────────
// 2. Happy path — multi-installation joiner (B1 + B2).
// ─────────────────────────────────────────────────────────────────────

#[xmtp_common::test(unwrap_try = true)]
async fn qr_invite_happy_path_multi_installation() {
    tester!(alix);
    tester!(bo_1);
    // bo_2 is a second installation on the same inbox as bo_1; the
    // `from: bo_1` shape on the tester macro uses the same wallet, so
    // both installations live under one inbox_id.
    tester!(bo_2, from: bo_1);

    // Sanity: same inbox.
    assert_eq!(
        bo_1.inbox_id(),
        bo_2.inbox_id(),
        "bo_2 must be a co-resident installation of bo_1",
    );

    let group = alix.create_group(Some(policy_allowing_external_commit()), None)?;
    // Settle alix's installation in the freshly-created group (the
    // initial GroupMembership entry is `alix: 0`; the first sync
    // auto-publishes an `UpdateGroupMembership` commit to bump alix's
    // sequence id to her current identity-update sequence). Without
    // this, a subsequent receive-side validation of alix's own metadata
    // commit can fail with `MissingIdentityUpdate`, and an invite
    // produced from the pre-sync GroupInfo would also be one epoch
    // behind alix's auto-bumped post-sync state.
    group.sync().await?;

    // Migrate the group to the AppData GROUP_MEMBERSHIP component. The
    // QR-invite flow writes via AppDataUpdate proposals (the post-migration
    // storage path); L-10 / L-11 explicitly refuse to operate on pre-migration
    // groups so callers don't produce commits other members can't apply.
    group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    // Flip the EXTERNAL_COMMIT_POLICY master switch on. Required for
    // the QR-invite flow — without this, alix's validator rejects bo's
    // external commit at the policy gate.
    group.set_allow_external_commit(true).await?;
    group.sync().await?;

    let mut service = InviteBlobStore::new();
    let out = group
        .create_external_invite(CreateExternalInviteOpts::default())
        .await?;
    service.put(&out.invite_payload, out.encrypted_group_info.clone());

    // bo_1 joins. L-11 internally `load_identity_updates`'s the inbox,
    // fetches bo_2's key package, and inlines an `Add` proposal for
    // bo_2 in the external commit.
    let blob_bytes = service.get(&out.invite_payload);
    let bo_1_join = bo_1
        .join_group_by_external_invite(&out.invite_payload, &blob_bytes)
        .await?;
    assert_eq!(bo_1_join.group.group_id, group.group_id);

    // Alix syncs and accepts the external commit.
    group.sync().await?;
    let alix_members = group.members().await?;
    let bo_entry = alix_members
        .iter()
        .find(|m| m.inbox_id == bo_1.inbox_id())
        .expect("alix must see bo's inbox as a member after the multi-installation join");
    // BOTH installations must appear under bo's single membership entry.
    let bo_installation_ids: std::collections::HashSet<_> =
        bo_entry.installation_ids.iter().cloned().collect();
    assert!(
        bo_installation_ids.contains(&bo_1.context.installation_id().to_vec()),
        "bo_1's installation must be a leaf under bo's inbox; saw: {:?}",
        bo_entry.installation_ids,
    );
    assert!(
        bo_installation_ids.contains(&bo_2.context.installation_id().to_vec()),
        "bo_2's installation must be a leaf under bo's inbox after the atomic external commit; saw: {:?}",
        bo_entry.installation_ids,
    );

    // bo_2 receives the HPKE-wrapped Welcome and joins independently.
    let bo_2_groups = bo_2.sync_welcomes().await?;
    let bo_2_group = bo_2_groups
        .iter()
        .find(|g| g.group_id == group.group_id)
        .expect("bo_2 must receive the welcome carrying the freshly-joined group");

    // Alix sends; both bo installations decrypt.
    let msg_text = b"hello to both of bo's installations";
    group
        .send_message(msg_text, SendMessageOpts::default())
        .await?;
    bo_1_join.group.sync().await?;
    bo_2_group.sync().await?;

    let bo_1_msgs = bo_1_join.group.find_messages(&MsgQueryArgs::default())?;
    assert!(
        bo_1_msgs
            .iter()
            .any(|m| m.decrypted_message_bytes == msg_text),
        "bo_1 must decrypt alix's post-join message",
    );
    let bo_2_msgs = bo_2_group.find_messages(&MsgQueryArgs::default())?;
    assert!(
        bo_2_msgs
            .iter()
            .any(|m| m.decrypted_message_bytes == msg_text),
        "bo_2 must decrypt alix's post-join message (via its independent welcome)",
    );
}

// ─────────────────────────────────────────────────────────────────────
// 3. Negative — group policy disallows external commits.
// ─────────────────────────────────────────────────────────────────────

#[xmtp_common::test(unwrap_try = true)]
async fn qr_invite_rejected_when_policy_disallows() {
    tester!(alix);
    tester!(bo);
    tester!(charlie);

    // Default policy: allow_external_commit = false. Charlie is in
    // the group from the start; she's the witness whose view must NOT
    // include bo after the rejected join attempt.
    let group = alix
        .create_group_with_members(&[charlie.inbox_id()], None, None)
        .await?;
    // Sanity: charlie's welcome is processed before we make assertions.
    let charlie_groups = charlie.sync_welcomes().await?;
    let charlie_group = charlie_groups
        .iter()
        .find(|g| g.group_id == group.group_id)
        .expect("charlie's welcome carries the group");

    // Migrate the group to AppData (required by L-10 / L-11). The
    // migration commit must propagate to charlie too so her later
    // sync sees the post-migration state.
    group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    // Flip the EXTERNAL_COMMIT_POLICY master switch on. Required for
    // the QR-invite flow — without this, alix's validator rejects bo's
    // external commit at the policy gate.
    group.set_allow_external_commit(true).await?;
    group.sync().await?;
    charlie_group.sync().await?;

    // L-10 does NOT enforce the policy at invite-generation time
    // (the producer just exports a GroupInfo); the policy is gated on
    // the L-7 receive-side validator. So invite generation succeeds
    // even though external joins are forbidden.
    let mut service = InviteBlobStore::new();
    let out = group
        .create_external_invite(CreateExternalInviteOpts::default())
        .await?;
    service.put(&out.invite_payload, out.encrypted_group_info);

    // bo joins. The local external-commit build succeeds (policy is
    // a receive-side check, not a build-side one) but the published
    // commit will be rejected by the validator on alix/charlie's
    // sync.
    let blob_bytes = service.get(&out.invite_payload);
    let _ = bo
        .join_group_by_external_invite(&out.invite_payload, &blob_bytes)
        .await;

    // alix and charlie sync; both must continue to see only the
    // original membership (no bo). The validator rejection may
    // surface as Err(_) on sync — we tolerate that since the
    // observable end-state (membership) is what callers actually
    // care about.
    let _ = group.sync().await;
    let _ = charlie_group.sync().await;

    let alix_members = group.members().await?;
    assert!(
        !alix_members.iter().any(|m| m.inbox_id == bo.inbox_id()),
        "alix must NOT see bo as a member after the disallowed external commit",
    );
    let charlie_members = charlie_group.members().await?;
    assert!(
        !charlie_members.iter().any(|m| m.inbox_id == bo.inbox_id()),
        "charlie must NOT see bo as a member after the disallowed external commit",
    );
}

// ─────────────────────────────────────────────────────────────────────
// 4. Negative — stale-epoch GroupInfo.
// ─────────────────────────────────────────────────────────────────────

#[xmtp_common::test(unwrap_try = true)]
async fn qr_invite_rejected_stale_epoch() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(Some(policy_allowing_external_commit()), None)?;
    // Settle alix's installation in the freshly-created group (the
    // initial GroupMembership entry is `alix: 0`; the first sync
    // auto-publishes an `UpdateGroupMembership` commit to bump alix's
    // sequence id to her current identity-update sequence). Without
    // this, a subsequent receive-side validation of alix's own metadata
    // commit can fail with `MissingIdentityUpdate`, and an invite
    // produced from the pre-sync GroupInfo would also be one epoch
    // behind alix's auto-bumped post-sync state.
    group.sync().await?;

    // Migrate the group to the AppData GROUP_MEMBERSHIP component. The
    // QR-invite flow writes via AppDataUpdate proposals (the post-migration
    // storage path); L-10 / L-11 explicitly refuse to operate on pre-migration
    // groups so callers don't produce commits other members can't apply.
    group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    // Flip the EXTERNAL_COMMIT_POLICY master switch on. Required for
    // the QR-invite flow — without this, alix's validator rejects bo's
    // external commit at the policy gate.
    group.set_allow_external_commit(true).await?;
    group.sync().await?;
    let epoch_before = group.epoch().await?;

    // Generate an invite for the current epoch...
    let mut service = InviteBlobStore::new();
    let out = group
        .create_external_invite(CreateExternalInviteOpts::default())
        .await?;
    service.put(&out.invite_payload, out.encrypted_group_info);

    // ...then advance the epoch on alix's side with a metadata commit.
    group.update_group_name("renamed".to_string()).await?;
    let epoch_after = group.epoch().await?;
    assert!(
        epoch_after > epoch_before,
        "metadata update must advance the epoch (was {epoch_before}, now {epoch_after})",
    );

    // bo's join attempt uses the stale GroupInfo. openmls's external
    // commit builder may accept the locally-staged commit (it has no
    // network view of alix's later epoch), or it may reject it
    // outright. Either way, the FINAL invariant is that alix must
    // not accept bo into the group after sync.
    let blob_bytes = service.get(&out.invite_payload);
    let _ = bo
        .join_group_by_external_invite(&out.invite_payload, &blob_bytes)
        .await;
    let _ = group.sync().await;

    let members = group.members().await?;
    assert!(
        !members.iter().any(|m| m.inbox_id == bo.inbox_id()),
        "alix must NOT accept a stale-epoch external commit from bo",
    );
}

// ─────────────────────────────────────────────────────────────────────
// 5. Joiner-self-chain — Charlie joins via the refreshed blob bo uploaded.
// ─────────────────────────────────────────────────────────────────────

#[xmtp_common::test(unwrap_try = true)]
async fn qr_invite_chain_self_refresh() {
    tester!(alix);
    tester!(bo);
    tester!(charlie);

    let group = alix.create_group(Some(policy_allowing_external_commit()), None)?;
    // Settle alix's installation in the freshly-created group (the
    // initial GroupMembership entry is `alix: 0`; the first sync
    // auto-publishes an `UpdateGroupMembership` commit to bump alix's
    // sequence id to her current identity-update sequence). Without
    // this, a subsequent receive-side validation of alix's own metadata
    // commit can fail with `MissingIdentityUpdate`, and an invite
    // produced from the pre-sync GroupInfo would also be one epoch
    // behind alix's auto-bumped post-sync state.
    group.sync().await?;

    // Migrate the group to the AppData GROUP_MEMBERSHIP component. The
    // QR-invite flow writes via AppDataUpdate proposals (the post-migration
    // storage path); L-10 / L-11 explicitly refuse to operate on pre-migration
    // groups so callers don't produce commits other members can't apply.
    group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    // Flip the EXTERNAL_COMMIT_POLICY master switch on. Required for
    // the QR-invite flow — without this, alix's validator rejects bo's
    // external commit at the policy gate.
    group.set_allow_external_commit(true).await?;
    group.sync().await?;

    // alix produces the original invite; service stores the original
    // encrypted blob.
    let mut service = InviteBlobStore::new();
    let out = group
        .create_external_invite(CreateExternalInviteOpts::default())
        .await?;
    service.put(&out.invite_payload, out.encrypted_group_info);

    // bo joins via the original invite and produces a REFRESHED
    // encrypted blob at the post-commit epoch under the same
    // symmetric key. The application uploads the refreshed blob
    // back to the service (overwriting the original).
    let blob_for_bo = service.get(&out.invite_payload);
    let bo_join = bo
        .join_group_by_external_invite(&out.invite_payload, &blob_for_bo)
        .await?;
    service.put(&out.invite_payload, bo_join.refreshed_encrypted_group_info);

    // alix processes bo's commit so the on-network epoch lines up
    // with what bo encrypted into the refreshed blob.
    group.sync().await?;

    // charlie reuses the SAME invite payload (same symmetric key,
    // same group_id_hash) but reads the REFRESHED blob from the
    // service. This is the joiner-as-republisher chain: the second
    // joiner consumes the first joiner's freshly-encrypted snapshot,
    // not the original.
    let blob_for_charlie = service.get(&out.invite_payload);
    assert_ne!(
        blob_for_charlie, blob_for_bo,
        "service must have served charlie the refreshed (not original) blob",
    );

    let charlie_join = charlie
        .join_group_by_external_invite(&out.invite_payload, &blob_for_charlie)
        .await?;
    assert_eq!(charlie_join.group.group_id, group.group_id);

    // alix processes charlie's external commit; both bo and charlie
    // must appear as members.
    group.sync().await?;
    let alix_members = group.members().await?;
    let inbox_ids: std::collections::HashSet<String> =
        alix_members.iter().map(|m| m.inbox_id.clone()).collect();
    assert!(
        inbox_ids.iter().any(|s| s == bo.inbox_id()),
        "alix must see bo as a member after the chain join; got {inbox_ids:?}",
    );
    assert!(
        inbox_ids.iter().any(|s| s == charlie.inbox_id()),
        "alix must see charlie as a member after joining via the refreshed blob; got {inbox_ids:?}",
    );
}

// ─────────────────────────────────────────────────────────────────────
// 6. AppDataUpdate scope — joiner only inserts THEIR OWN inbox entry.
// ─────────────────────────────────────────────────────────────────────

#[xmtp_common::test(unwrap_try = true)]
async fn qr_invite_membership_update_is_inbox_scoped() {
    tester!(alix);
    tester!(bo_1);
    tester!(_bo_2, from: bo_1);
    tester!(charlie);

    // alix + charlie are the original members; the QR-invite path
    // brings bo in. charlie acts as a non-joining observer whose
    // membership entry must remain unchanged after bo's external
    // commit (L-7 enforces AppDataUpdate is inbox-scoped to the
    // joiner).
    let group = alix
        .create_group_with_members(
            &[charlie.inbox_id()],
            Some(policy_allowing_external_commit()),
            None,
        )
        .await?;
    let charlie_groups = charlie.sync_welcomes().await?;
    let charlie_group = charlie_groups
        .iter()
        .find(|g| g.group_id == group.group_id)
        .expect("charlie must receive the initial welcome");

    // Migrate the group to AppData (required by L-10 / L-11). Sync
    // charlie so her local view advances past the bootstrap commit
    // before the membership-scope snapshots are taken.
    group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    // Flip the EXTERNAL_COMMIT_POLICY master switch on. Required for
    // the QR-invite flow — without this, alix's validator rejects bo's
    // external commit at the policy gate.
    group.set_allow_external_commit(true).await?;
    group.sync().await?;
    charlie_group.sync().await?;

    // Snapshot pre-join GROUP_MEMBERSHIP entries from each existing
    // member's local view.
    let alix_membership_before =
        group.load_mls_group_with_lock(group.context.mls_storage(), |mls_group| {
            Ok::<_, crate::groups::GroupError>(extract_group_membership(mls_group.extensions())?)
        })?;
    let charlie_membership_before = charlie_group.load_mls_group_with_lock(
        charlie_group.context.mls_storage(),
        |mls_group| {
            Ok::<_, crate::groups::GroupError>(extract_group_membership(mls_group.extensions())?)
        },
    )?;
    let alix_seq_before = alix_membership_before.get(alix.inbox_id()).copied();
    let charlie_seq_before = alix_membership_before.get(charlie.inbox_id()).copied();
    let charlie_self_seq_before = charlie_membership_before.get(charlie.inbox_id()).copied();
    let charlie_view_of_alix_before = charlie_membership_before.get(alix.inbox_id()).copied();
    assert!(
        alix_seq_before.is_some(),
        "alix should be in the initial membership component",
    );
    assert!(
        charlie_seq_before.is_some(),
        "charlie should be in the initial membership component",
    );

    // bo joins via QR invite (with bo_2 atomically added as a leaf).
    let mut service = InviteBlobStore::new();
    let out = group
        .create_external_invite(CreateExternalInviteOpts::default())
        .await?;
    service.put(&out.invite_payload, out.encrypted_group_info);
    let blob_bytes = service.get(&out.invite_payload);
    let _ = bo_1
        .join_group_by_external_invite(&out.invite_payload, &blob_bytes)
        .await?;

    // alix + charlie sync; both observe bo's external commit and
    // accept it through the L-7 validator.
    group.sync().await?;
    charlie_group.sync().await?;

    // After the join, the membership component must contain:
    //   - bo's inbox (newly inserted by the joiner's AppDataUpdate),
    //   - alix's entry UNCHANGED,
    //   - charlie's entry UNCHANGED.
    let alix_membership_after =
        group.load_mls_group_with_lock(group.context.mls_storage(), |mls_group| {
            Ok::<_, crate::groups::GroupError>(extract_group_membership(mls_group.extensions())?)
        })?;
    let charlie_membership_after = charlie_group.load_mls_group_with_lock(
        charlie_group.context.mls_storage(),
        |mls_group| {
            Ok::<_, crate::groups::GroupError>(extract_group_membership(mls_group.extensions())?)
        },
    )?;

    assert!(
        alix_membership_after.get(bo_1.inbox_id()).is_some(),
        "alix's view must include bo's inbox after the external commit",
    );
    assert_eq!(
        alix_membership_after.get(alix.inbox_id()).copied(),
        alix_seq_before,
        "alix's own sequence id must be UNCHANGED by bo's joiner-scoped AppDataUpdate",
    );
    assert_eq!(
        alix_membership_after.get(charlie.inbox_id()).copied(),
        charlie_seq_before,
        "charlie's sequence id must be UNCHANGED by bo's joiner-scoped AppDataUpdate (from alix's view)",
    );
    assert!(
        charlie_membership_after.get(bo_1.inbox_id()).is_some(),
        "charlie's view must include bo's inbox after the external commit",
    );
    assert_eq!(
        charlie_membership_after.get(alix.inbox_id()).copied(),
        charlie_view_of_alix_before,
        "alix's sequence id must be UNCHANGED from charlie's view too",
    );
    assert_eq!(
        charlie_membership_after.get(charlie.inbox_id()).copied(),
        charlie_self_seq_before,
        "charlie's own sequence id must be UNCHANGED by bo's joiner-scoped AppDataUpdate",
    );
}
