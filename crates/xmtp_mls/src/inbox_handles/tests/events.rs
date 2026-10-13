//! `handle.changed`: emitted from the write that stores a newer association
//! state, only when the display handle changes.

use super::{
    ClaimSigner::Installation, claim_for_test, new_inbox_state, random_handle, state_with_handles,
};
use crate::{
    client::ClientError, context::XmtpSharedContext, identity_updates::store_association_state,
    tester,
};
use xmtp_events::{ClientEvent, EventEnvelope, EventFilter, EventKind, HandleChanged};
use xmtp_id::associations::AssociationState;

fn handle_events<I>(drained: Vec<EventEnvelope<I>>) -> Vec<HandleChanged> {
    drained
        .into_iter()
        .filter_map(|event| match event.client {
            Some(ClientEvent::HandleChanged(change)) => Some(change),
            _ => None,
        })
        .collect()
}

fn changed(state: &AssociationState) -> HandleChanged {
    HandleChanged {
        inbox_id: state.inbox_id().to_owned(),
        handle: state
            .display_handle()
            .expect("a display handle")
            .to_string(),
    }
}

// verifies: EVENT-058
#[xmtp_common::test(unwrap_try = true)]
async fn first_state_emits_no_handle_event() {
    tester!(alix);
    tester!(bo);
    let handle = random_handle();
    claim_for_test(&alix, &handle, Installation).await?;
    let events = bo
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::HandleChanged]), Some(10));

    // Bo holds no state for alix before this load.
    let states = bo.inbox_addresses(true, vec![alix.inbox_id()]).await?;

    assert_eq!(states[0].display_handle(), Some(&handle));
    assert!(handle_events(events.drain()).is_empty());
}

// verifies: EVENT-058
#[xmtp_common::test(unwrap_try = true)]
async fn peer_claim_emits_one_handle_changed() {
    tester!(alix);
    tester!(bo);
    let unclaimed = bo.inbox_addresses(true, vec![alix.inbox_id()]).await?;
    assert_eq!(unclaimed[0].display_handle(), None);
    let bo_events = bo
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::HandleChanged]), Some(10));
    let alix_events = alix
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::HandleChanged]), Some(10));

    let handle = random_handle();
    claim_for_test(&alix, &handle, Installation).await?;
    bo.inbox_addresses(true, vec![alix.inbox_id()]).await?;

    let expected = HandleChanged {
        inbox_id: alix.inbox_id().to_owned(),
        handle: handle.to_string(),
    };
    assert_eq!(handle_events(bo_events.drain()), vec![expected.clone()]);
    // The own inbox has the event too.
    assert_eq!(handle_events(alix_events.drain()), vec![expected]);
}

// verifies: EVENT-058
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn rename_emits_new_display_handle() {
    use super::{ClaimSigner::Wallet, PAST_CLAIM_INTERVAL};
    use crate::utils::test::backend::EphemeralBackend;
    // An inbox claims more than once, so the claims stay off the shared backend.
    let backend = EphemeralBackend::start("").await?;
    tester!(alix, backend: &backend, disable_workers);
    tester!(bo, backend: &backend, disable_workers);
    let first = random_handle();
    let second = random_handle();
    claim_for_test(&alix, &first, Installation).await?;
    bo.inbox_addresses(true, vec![alix.inbox_id()]).await?;
    let events = bo
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::HandleChanged]), Some(10));

    backend
        .age_identity_history(alix.inbox_id(), PAST_CLAIM_INTERVAL)
        .await?;
    claim_for_test(&alix, &second, Installation).await?;
    backend
        .age_identity_history(alix.inbox_id(), PAST_CLAIM_INTERVAL)
        .await?;
    claim_for_test(&alix, &first, Wallet).await?;
    let switched_back = bo.inbox_addresses(true, vec![alix.inbox_id()]).await?;
    assert_eq!(switched_back[0].handles(), [second.clone(), first.clone()]);
    assert!(
        handle_events(events.drain()).is_empty(),
        "new handles with the same display handle are not a change"
    );

    backend
        .age_identity_history(alix.inbox_id(), PAST_CLAIM_INTERVAL)
        .await?;
    claim_for_test(&alix, &second, Wallet).await?;
    bo.inbox_addresses(true, vec![alix.inbox_id()]).await?;
    assert_eq!(
        handle_events(events.drain()),
        vec![HandleChanged {
            inbox_id: alix.inbox_id().to_owned(),
            handle: second.to_string(),
        }]
    );
}

// verifies: EVENT-058
#[xmtp_common::test(unwrap_try = true)]
async fn older_state_stored_later_emits_nothing() {
    tester!(alix, disable_workers);
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

// verifies: EVENT-059
#[xmtp_common::test(unwrap_try = true)]
async fn handle_events_for_one_inbox_keep_sequence_order() {
    tester!(alix, disable_workers);
    let context = &alix.context;
    let events = context
        .events()
        .subscribe(EventFilter::new([EventKind::HandleChanged]), Some(10));

    // Two tasks store sequence ids 5 and 7 of one inbox. The client
    // serializes the two writes, so each round fixes one of the two commit
    // orders. Each task reads the events right after its store returns.
    for older_first in [true, false] {
        let base = new_inbox_state();
        store_association_state(context, 3, &base)?;
        let five = state_with_handles(&base, &["five"]);
        let seven = state_with_handles(&base, &["five", "seven"]);
        let (first, second) = if older_first {
            ((5, &five), (7, &seven))
        } else {
            ((7, &seven), (5, &five))
        };
        let first_stored = tokio::sync::Notify::new();
        let first_task = async {
            store_association_state(context, first.0, first.1)?;
            let seen = handle_events(events.drain());
            first_stored.notify_one();
            Ok::<_, ClientError>(seen)
        };
        let second_task = async {
            first_stored.notified().await;
            store_association_state(context, second.0, second.1)?;
            Ok::<_, ClientError>(handle_events(events.drain()))
        };
        let (first_seen, second_seen) = tokio::join!(first_task, second_task);

        if older_first {
            assert_eq!(first_seen?, vec![changed(&five)]);
            assert_eq!(second_seen?, vec![changed(&seven)]);
        } else {
            assert_eq!(first_seen?, vec![changed(&seven)]);
            assert!(
                second_seen?.is_empty(),
                "a store of 5 after 7 emits nothing"
            );
        }
    }
}
