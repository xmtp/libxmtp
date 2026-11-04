//! The leaves of `groups/mls_sync/publish/tests/uncommittable_proposals.rs` that need the client.

use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;

/// Receivers reject a commit that leaves another inbox in a sync group, so a
/// pending add of one must stay out of every commit. Otherwise it blocks
/// every later commit to the group.
// verifies: GMOD-047, SYNC-010
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn a_pending_add_of_another_inbox_to_a_sync_group_is_omitted()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::intents::{ProposeMemberUpdateIntentData, QueueIntent};

    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    tester!(eve, disable_workers);
    let group = alix1.device_sync_client().get_sync_group().await?;
    alix2.sync_welcomes().await?;
    let alix2_group = alix2.group(&group.group_id)?;
    let intent = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![eve.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(intent.id).await?;
    alix2_group.sync().await?;
    let pending = || alix2_group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()));
    assert!(pending()? > 0, "alix2 must hold the proposal");
    let epoch = alix2_group.epoch().await?;

    alix2_group.key_update().await?;
    alix2_group
        .send_message(b"hello", SendMessageOpts::default())
        .await?;

    assert!(alix2_group.epoch().await? > epoch);
    assert_eq!(pending()?, 0);
    let members = alix2_group.members().await?;
    assert!(members.iter().all(|m| m.inbox_id != eve.inbox_id()));
    Ok(())
}
