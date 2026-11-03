//! Handle tests on private state: the claim cap check and the handle events
//! of stored association states.

mod claims;
mod events;

use xmtp_id::associations::{AssociationState, Identifier, handle::Handle};

/// `base` after claims of `handles`, in order. It is built in memory, for
/// tests that store states directly.
fn state_with_handles(base: &AssociationState, handles: &[&str]) -> AssociationState {
    handles.iter().fold(base.clone(), |state, handle| {
        state.claim_handle(Handle::parse(handle).expect("test handles match the grammar"))
    })
}

/// A new inbox's association state, with no handles.
fn new_inbox_state() -> AssociationState {
    AssociationState::new(Identifier::rand_ethereum(), 0, None).expect("a new inbox state")
}
