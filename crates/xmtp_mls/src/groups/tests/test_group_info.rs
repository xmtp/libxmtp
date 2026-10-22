//! A commit never exports a GroupInfo. Its group context holds the group
//! root, which may leave the client only inside a Welcome.

use crate::{
    context::XmtpSharedContext,
    groups::{
        GroupError, app_data::stage_app_data_proposals_and_commit,
        mls_sync::generate_prepared_commit,
    },
    state_tx::state_write,
    tester,
};
use openmls::messages::proposals::AppDataUpdateOperation;
use xmtp_db::TransactionOutcome::Continue;
use xmtp_mls_common::app_data::component_id::ComponentId;

/// XMTP groups use the ratchet-tree extension, so OpenMLS exports a
/// GroupInfo with every commit unless the builder turns it off. The AppData
/// commit builder must turn it off.
// verifies: GMOD-042
#[xmtp_common::test(unwrap_try = true)]
async fn app_data_commit_exports_no_group_info() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let signer = &alix.context.identity().installation_keys;
    let updates = vec![(
        ComponentId::GROUP_NAME,
        AppDataUpdateOperation::Update(b"renamed".to_vec().into()),
    )];
    let exported = state_write(alix.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, storage| {
            let ((_proposals, bundle), _staged_commit, _epoch) = generate_prepared_commit(
                storage,
                mls_group,
                |mls_group, provider| -> Result<_, GroupError> {
                    Ok(stage_app_data_proposals_and_commit(
                        mls_group,
                        provider,
                        signer,
                        &[],
                        updates,
                    )?)
                },
            )?;
            let (_commit, _welcome, group_info) = bundle.into_messages();
            Ok::<_, GroupError>(Continue(group_info.is_some()))
        })
    })?
    .into_continued();
    assert!(!exported, "the commit exported a GroupInfo");
}
