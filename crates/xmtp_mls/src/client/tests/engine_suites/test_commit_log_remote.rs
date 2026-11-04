//! The leaves of `groups/tests/test_commit_log_remote.rs` that need the client.

use crate::context::XmtpSharedContext;
use crate::groups::MlsGroup;
use crate::tester;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::prelude::*;
use xmtp_mls_group_keys::group_keys::GroupKeys;
use xmtp_mls_group_state::app_data::group_root::group_keys_from_extensions;

#[xmtp_common::test(unwrap_try = true)]
async fn test_should_publish_commit_log() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None).unwrap();
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();

    let binding = bo.list_conversations(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    assert_eq!(bo_group.group.group_id, alix_group.group_id);

    let alix_should_publish_commit_log_groups = alix
        .find_groups(GroupQueryArgs {
            should_publish_commit_log: Some(true),
            ..Default::default()
        })
        .unwrap();

    let bo_should_publish_commit_log_groups = bo
        .find_groups(GroupQueryArgs {
            should_publish_commit_log: Some(true),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(alix_should_publish_commit_log_groups.len(), 1);
    assert_eq!(bo_should_publish_commit_log_groups.len(), 0);
}

/// The keys of a conversation's group root, as its commit-log worker derives
/// them.
fn keys_of(group: &MlsGroup<impl XmtpSharedContext>) -> GroupKeys {
    group_keys_from_extensions(group.load_group_context().unwrap().extensions()).unwrap()
}

/// Every conversation stores its commit-log id at creation and at join. It
/// is the public key of the commit-log signing key the group root derives,
/// so every member stores the same one, and it is not the group id.
// verifies: GMOD-041
#[xmtp_common::test(unwrap_try = true)]
async fn the_commit_log_id_is_stored_at_creation_and_join() {
    tester!(alix, with_commit_log_worker: false);
    tester!(bo, with_commit_log_worker: false);

    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    bo.sync_welcomes().await?;
    for created in [&group, &dm] {
        let joined = bo.group(&created.group_id)?;
        let commit_log_id = keys_of(created).commit_log_id().to_vec();
        assert_ne!(commit_log_id, created.group_id.to_vec());
        assert_eq!(keys_of(&joined).commit_log_id().to_vec(), commit_log_id);
        let created_row = alix.context.db().find_group(&created.group_id)??;
        assert_eq!(
            created_row.commit_log_public_key,
            Some(commit_log_id.clone()),
            "stored at creation"
        );
        let joined_row = bo.context.db().find_group(&created.group_id)??;
        assert_eq!(
            joined_row.commit_log_public_key,
            Some(commit_log_id),
            "stored at join"
        );
    }
}
