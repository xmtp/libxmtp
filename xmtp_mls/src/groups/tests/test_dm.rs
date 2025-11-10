use std::sync::Arc;

use tracing::error;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::prelude::*;

use crate::tester;

/// Test case: If two users are talking in a DM, and one user
/// creates a new installation and creates a new DM before being
/// welcomed into the old DM, that new DM group should be consented.
#[xmtp_common::test(unwrap_try = true)]
async fn auto_consent_dms_for_new_installations() {
    tester!(alix);
    tester!(bo1);
    // Alix and bo are talking fine in a DM
    alix.test_talk_in_dm_with(&bo1).await?;

    tester!(bo2, from: bo1);

    // Bo creates a new installation and immediately creates a new DM with alix
    let bo2_dm = bo2
        .find_or_create_dm_by_inbox_id(alix.inbox_id(), None)
        .await?;

    // Alix pulls down the new DM from bo
    alix.sync_welcomes().await?;

    // That DM should be already consented, since alix consented with bo in another DM
    let consent = alix
        .get_consent_state(ConsentType::ConversationId, hex::encode(bo2_dm.group_id))
        .await?;
    assert_eq!(consent, ConsentState::Allowed);
}

/// Test case: If a second installation syncs the consent state for a DM
/// before processing the welcome, the welcome should succeed rather than
/// aborting on a unique constraint error.
#[xmtp_common::test(unwrap_try = true)]
async fn test_dm_welcome_with_preexisting_consent() {
    tester!(alix);
    tester!(bo1);
    // Alix and bo are talking fine in a DM
    let (a_group, _) = alix.test_talk_in_dm_with(&bo1).await?;

    tester!(bo2, from: bo1);

    // Mock device sync - the consent record is processed on Bo2 before
    // the welcome is processed.
    let cr = StoredConsentRecord::new(
        ConsentType::ConversationId,
        ConsentState::Allowed,
        hex::encode(&a_group.group_id),
    );
    bo2.context.db().insert_newer_consent_record(cr)?;
    // Now bo2 processes the welcome
    bo1.find_or_create_dm_by_inbox_id(alix.inbox_id(), None)
        .await?
        .update_installations()
        .await?;
    bo2.sync_welcomes().await?;

    // The welcome should succeed
    assert_eq!(
        bo2.find_or_create_dm_by_inbox_id(alix.inbox_id(), None)
            .await?
            .group_id,
        a_group.group_id
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn create_14k_groups() {
    use indicatif::ProgressBar;
    use tracing::error;

    tester!(alix);
    tester!(bo);

    let num_groups = 14_000;
    let prog = ProgressBar::new(num_groups);
    for _ in 0..num_groups {
        error!("a");
        bo.create_group_with_inbox_ids(&[alix.inbox_id()], None, None)
            .await?;
        error!("b");
        alix.sync_welcomes().await?;
        let num_groups = alix.find_groups(Default::default())?.len();
        tracing::error!("{num_groups}");
        prog.inc(1);
    }

    let snap = alix.dump_db();
    std::fs::write("alix.db3", &snap)?;
    let snap = bo.dump_db();
    std::fs::write("bo.db3", &snap)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_14k_groups() {
    let snap = Arc::new(std::fs::read("alix.db3")?);
    tester!(alix, snapshot: snap);

    let groups = alix.find_groups(Default::default())?;
    error!("{}", groups.len());
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_snap() {
    tester!(alix);
    let snap = Arc::new(alix.dump_db());
    tester!(alix2, snapshot: snap);
}
