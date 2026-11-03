//! `handle.changed` is not emitted when an older association state is stored
//! after a newer one.

use super::{new_inbox_state, state_with_handles};
use crate::{
    context::CoreContext, identity_lifecycle::test_support::IdentityTestPeer,
    identity_updates::store_association_state,
};
use xmtp_events::{ClientEvent, EventEnvelope, EventFilter, EventKind, HandleChanged};

// verifies: EVENT-058
#[xmtp_common::test(unwrap_try = true)]
async fn older_state_stored_later_emits_nothing() {
    let alix = IdentityTestPeer::new().await;
    let base = new_inbox_state();
    store_association_state(&alix.context, 3, &base)?;
    let newest = state_with_handles(&base, &["one", "two", "three"]);
    store_association_state(&alix.context, 7, &newest)?;
    let events = alix
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::HandleChanged]), Some(10));

    // Each older state has a display handle that the newest state does not.
    store_association_state(&alix.context, 5, &state_with_handles(&base, &["one"]))?;
    store_association_state(
        &alix.context,
        6,
        &state_with_handles(&base, &["one", "two"]),
    )?;

    assert!(handle_events(events.drain()).is_empty());
}

fn handle_events<I>(drained: Vec<EventEnvelope<I>>) -> Vec<HandleChanged> {
    drained
        .into_iter()
        .filter_map(|event| match event.client {
            Some(ClientEvent::HandleChanged(change)) => Some(change),
            _ => None,
        })
        .collect()
}
