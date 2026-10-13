//! Resolving and storing association states. A client stores each state it
//! computes through one write that also reports a changed display handle.

use crate::{client::ClientError, context::XmtpSharedContext};
use xmtp_db::{
    StorageError, TransactionOutcome, identity_update::StoredIdentityUpdate, prelude::*,
};
use xmtp_id::{
    associations::{
        AssociationError, AssociationState, get_state, unverified::UnverifiedIdentityUpdate,
        verify_updates,
    },
    scw_verifier::SmartContractSignatureVerifier,
};

use crate::identity::IdentityExt;

/// An association state read from the cache, or one computed from the stored
/// identity updates that the caller must store.
pub(super) enum ResolvedState {
    Cached(AssociationState),
    Computed {
        sequence_id: i64,
        state: AssociationState,
    },
}

/// Read the association state of `inbox_id` at `to_sequence_id`, or at the
/// newest stored update, from the cache. On a miss, compute it from the
/// stored updates and leave the store to the caller.
pub(super) async fn resolve_association_state(
    conn: &impl DbQuery,
    inbox_id: &str,
    to_sequence_id: Option<i64>,
    scw_verifier: &impl SmartContractSignatureVerifier,
) -> Result<ResolvedState, ClientError> {
    let updates = conn.get_identity_updates(inbox_id, None, to_sequence_id)?;
    let last_sequence_id = updates
        .last()
        .ok_or::<ClientError>(AssociationError::MissingIdentityUpdate.into())?
        .sequence_id;
    if let Some(to_sequence_id) = to_sequence_id
        && to_sequence_id != last_sequence_id
    {
        return Err(AssociationError::MissingIdentityUpdate.into());
    }

    if let Some(association_state) = conn.read_from_cache(inbox_id, last_sequence_id)? {
        return Ok(ResolvedState::Cached(
            association_state.try_into().map_err(StorageError::from)?,
        ));
    }

    let unverified_updates = updates
        .into_iter()
        // deserialize identity update payload
        .map(StoredIdentityUpdate::to_unverified)
        .collect::<Result<Vec<UnverifiedIdentityUpdate>, AssociationError>>()?;
    let updates = verify_updates(unverified_updates, scw_verifier).await?;

    Ok(ResolvedState::Computed {
        sequence_id: last_sequence_id,
        state: get_state(updates)?,
    })
}

/// [`get_association_state_with_verifier`](super::get_association_state_with_verifier)
/// for a client: a computed state is
/// stored by [`store_association_state`], which emits `handle.changed`.
pub(crate) async fn get_association_state_for_client(
    context: &impl XmtpSharedContext,
    conn: &impl DbQuery,
    inbox_id: &str,
    to_sequence_id: Option<i64>,
) -> Result<AssociationState, ClientError> {
    let verifier = context.scw_verifier();
    match resolve_association_state(conn, inbox_id, to_sequence_id, &verifier).await? {
        ResolvedState::Cached(state) => Ok(state),
        ResolvedState::Computed { sequence_id, state } => {
            store_association_state(context, sequence_id, &state)?;
            Ok(state)
        }
    }
}

/// Store the association state of an inbox at `sequence_id` in the client's
/// cache. In the same write, read the newest state the client already holds
/// for the inbox, and emit `handle.changed` when the stored state changes its
/// display handle. Concurrent stores for one inbox are serialized by the
/// write, so their events follow their sequence ids.
// implements: EVENT-058, EVENT-059
pub(crate) fn store_association_state(
    context: &impl XmtpSharedContext,
    sequence_id: i64,
    state: &AssociationState,
) -> Result<(), ClientError> {
    use xmtp_events::{ClientEvent, EventWriter};
    let inbox_id = state.inbox_id();
    crate::state_tx::state_write_with_events(
        context.mls_storage(),
        context.events(),
        |tx, events| {
            let storage = tx.storage();
            let db = storage.db();
            let held = db
                .newest_cached_states(&[inbox_id])?
                .remove(inbox_id)
                .map(|(held_sequence, held)| {
                    AssociationState::try_from(held).map(|held| (held_sequence, held))
                })
                .transpose()
                .map_err(StorageError::from)?;
            db.write_to_cache(inbox_id.to_owned(), sequence_id, state.clone().into())?;
            if let Some(change) = handle_change(held.as_ref(), sequence_id, state) {
                events.emit(Some(ClientEvent::HandleChanged(change)), None);
            }
            Ok::<_, StorageError>(TransactionOutcome::Continue(()))
        },
    )?
    .into_continued();
    Ok(())
}

/// The `handle.changed` event for storing `state` at `sequence_id`, given
/// the newest state the client held for the inbox before the store. Storing
/// the first state of an inbox, or a state no newer than the held one, is not
/// a change.
fn handle_change(
    held: Option<&(i64, AssociationState)>,
    sequence_id: i64,
    state: &AssociationState,
) -> Option<xmtp_events::HandleChanged> {
    let (held_sequence, held) = held?;
    if sequence_id <= *held_sequence {
        return None;
    }
    let handle = state.display_handle()?;
    (held.display_handle() != Some(handle)).then(|| xmtp_events::HandleChanged {
        inbox_id: state.inbox_id().to_owned(),
        handle: handle.to_string(),
    })
}
