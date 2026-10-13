//! `handle.changed`: emitted from the write that stores a newer association
//! state, only when the display handle changes.

use super::{new_inbox_state, random_handle, state_with_handles};
use crate::{context::XmtpSharedContext, identity_updates::store_association_state, tester};
use xmtp_events::{ClientEvent, EventEnvelope, EventFilter, EventKind, HandleChanged};

fn handle_events<I>(drained: Vec<EventEnvelope<I>>) -> Vec<HandleChanged> {
    drained
        .into_iter()
        .filter_map(|event| match event.client {
            Some(ClientEvent::HandleChanged(change)) => Some(change),
            _ => None,
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn changed(state: &xmtp_id::associations::AssociationState) -> HandleChanged {
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
    alix.claim_handle(&handle).await?;
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
    alix.claim_handle(&handle).await?;
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
    use super::{PAST_CLAIM_INTERVAL, claim_with_wallet};
    use crate::utils::test::backend::EphemeralBackend;
    // An inbox claims more than once, so the claims stay off the shared backend.
    let backend = EphemeralBackend::start("").await?;
    tester!(alix, backend: &backend, disable_workers);
    tester!(bo, backend: &backend, disable_workers);
    let first = random_handle();
    let second = random_handle();
    alix.claim_handle(&first).await?;
    bo.inbox_addresses(true, vec![alix.inbox_id()]).await?;
    let events = bo
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::HandleChanged]), Some(10));

    backend
        .age_identity_history(alix.inbox_id(), PAST_CLAIM_INTERVAL)
        .await?;
    alix.claim_handle(&second).await?;
    backend
        .age_identity_history(alix.inbox_id(), PAST_CLAIM_INTERVAL)
        .await?;
    claim_with_wallet(&alix, &first).await?;
    let switched_back = bo.inbox_addresses(true, vec![alix.inbox_id()]).await?;
    assert_eq!(switched_back[0].handles(), [second.clone(), first.clone()]);
    assert!(
        handle_events(events.drain()).is_empty(),
        "new handles with the same display handle are not a change"
    );

    backend
        .age_identity_history(alix.inbox_id(), PAST_CLAIM_INTERVAL)
        .await?;
    claim_with_wallet(&alix, &second).await?;
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
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn handle_events_for_one_inbox_keep_sequence_order() {
    use crate::{client::ClientError, state_tx::postcommit_test_hook};
    use std::{sync::mpsc, time::Duration};
    /// How long the first writer waits, after its commit, for the second
    /// store to return. The second store cannot return before the first
    /// writer's events are out, so with ordered emission this always elapses.
    const SECOND_STORE_WAIT: Duration = Duration::from_millis(250);

    tester!(alix, disable_workers);
    let context = &alix.context;
    let events = context
        .events()
        .subscribe(EventFilter::new([EventKind::HandleChanged]), Some(10));
    let base = new_inbox_state();
    store_association_state(context, 3, &base)?;
    let five = state_with_handles(&base, &["five"]);
    let seven = state_with_handles(&base, &["five", "seven"]);

    // Two threads store sequence ids 5 and 7 of one inbox. The first writer
    // commits 5, then waits before its store returns while the second writer
    // stores 7. An event emitted outside the write would let 7 overtake 5.
    let (first_committed, first_has_committed) = mpsc::channel();
    let (second_stored, second_has_stored) = mpsc::channel::<()>();
    let (five_ref, seven_ref) = (&five, &seven);
    std::thread::scope(|scope| {
        let first = scope.spawn(move || {
            let _hold = postcommit_test_hook::install(move || {
                let _ = first_committed.send(());
                let _ = second_has_stored.recv_timeout(SECOND_STORE_WAIT);
            });
            store_association_state(context, 5, five_ref)
        });
        let second = scope.spawn(move || {
            first_has_committed
                .recv()
                .expect("the first writer commits");
            let stored = store_association_state(context, 7, seven_ref);
            let _ = second_stored.send(());
            stored
        });
        first.join().expect("the first writer finishes")?;
        second.join().expect("the second writer finishes")?;
        Ok::<_, ClientError>(())
    })?;
    assert_eq!(
        handle_events(events.drain()),
        vec![changed(&five), changed(&seven)]
    );

    // A store of 5 after 7 emits nothing.
    let base = new_inbox_state();
    store_association_state(context, 3, &base)?;
    let five = state_with_handles(&base, &["five"]);
    let seven = state_with_handles(&base, &["five", "seven"]);
    store_association_state(context, 7, &seven)?;
    store_association_state(context, 5, &five)?;
    assert_eq!(handle_events(events.drain()), vec![changed(&seven)]);
}
