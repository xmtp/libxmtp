//! Pending proposals a commit cannot carry.

use super::*;
use openmls::messages::proposals::AppDataUpdateOperation;
use xmtp_mls_common::{
    app_data::component_id::ComponentId, inbox_id::InboxId, tls_map::TlsMapDelta,
};

/// A commit covers each reference once, so a repeated reference must count
/// once in the pending state. Otherwise an Insert, a Delete, and the Insert
/// again would leave the key present for the sender and absent for every
/// receiver of the commit.
// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
async fn a_repeated_reference_counts_once() -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let owner = InboxId::from_hex(alix.inbox_id()).unwrap();
    let insert = TlsMapDelta::<InboxId, tls_codec::VLBytes>::new()
        .insert(owner, b"Alix".to_vec().into())
        .tls_serialize_detached()?;
    let delete = TlsMapDelta::<InboxId, tls_codec::VLBytes>::new()
        .delete(owner)
        .tls_serialize_detached()?;
    let signer = &group.context.identity().installation_keys;
    let (omitted, expected) = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let provider = XmtpOpenMlsProviderRef::new(storage);
            let mut references = Vec::new();
            for payload in [&insert, &delete] {
                let (_, reference) = mls
                    .propose_app_data_update(
                        &provider,
                        signer,
                        ComponentId::USER_DISPLAY_NAME.as_u16(),
                        AppDataUpdateOperation::Update(payload.clone().into()),
                    )
                    .map_err(GroupError::Proposal)?;
                references.push(reference);
            }
            let repeat = mls.pending_proposals().next().cloned().unwrap();
            mls.store_pending_proposal(storage, repeat)?;
            let omitted = selection::omit_uncommittable_proposals(mls, storage)?;
            Ok::<_, GroupError>(Continue((omitted, references)))
        })
    })?
    .into_continued();
    assert!(omitted);
    let stored: Vec<_> = group.with_group_snapshot(|mls| {
        Ok(mls
            .pending_proposals()
            .map(|queued| queued.proposal_reference_ref().clone())
            .collect())
    })?;
    assert_eq!(stored, expected);
    Ok(())
}
