//! Own-installation events that the client reports from identity loads and
//! registrations that store some pages and then fail.

use crate::builder::ClientBuilder;
use crate::context::XmtpSharedContext;
use crate::identity_updates::load_identity_updates;
use crate::identity_updates::test_support::{IdentityScript, ScriptedIdentity};
use crate::prelude::*;
use crate::subscriptions::policy::StreamPolicy;
use crate::tester;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::Notify;
use xmtp_events::{ClientEvent, EventFilter, EventKind};
use xmtp_id::associations::test_utils::add_wallet_signature;

type ScriptedContext = Arc<
    crate::context::XmtpMlsLocalContext<
        ScriptedIdentity,
        xmtp_db::DefaultStore,
        crate::utils::TestMlsStorage,
    >,
>;

/// A client of `base`'s inbox and database whose backend calls go through a
/// script, reading at most `max_fetched_rows` envelopes per page.
async fn scripted_client(
    base: &crate::utils::Tester,
    max_fetched_rows: u32,
) -> (crate::Client<ScriptedContext>, Arc<Mutex<IdentityScript>>) {
    let script = Arc::new(Mutex::new(IdentityScript::default()));
    let client = ClientBuilder::from_client(base.client.clone())
        .api_client(ScriptedIdentity {
            inner: base.context.api().api_client.raw_for_test().clone(),
            script: script.clone(),
        })
        .stream_policy(StreamPolicy {
            max_fetched_rows,
            ..Default::default()
        })
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await
        .expect("a scripted client");
    (client, script)
}

type Events = xmtp_events::Subscription<crate::subscriptions::internal::InternalEvent>;

fn own_installation_events<C: XmtpSharedContext>(client: &crate::Client<C>) -> Events {
    client.context.events().subscribe(
        EventFilter::new([
            EventKind::IdentityOwnInstallationAdded,
            EventKind::IdentityOwnInstallationRevoked,
        ]),
        Some(10),
    )
}

/// Each own-installation event since the last call: whether it adds or
/// revokes, and which installation.
fn drained(events: &Events) -> Vec<(bool, Vec<u8>)> {
    events
        .drain()
        .iter()
        .map(|event| match &event.client {
            Some(ClientEvent::IdentityOwnInstallationAdded(added)) => {
                (true, added.installation_key.clone())
            }
            Some(ClientEvent::IdentityOwnInstallationRevoked(revoked)) => {
                (false, revoked.installation_key.clone())
            }
            other => panic!("unexpected event {other:?}"),
        })
        .collect()
}

fn added(tester: &crate::utils::Tester) -> (bool, Vec<u8>) {
    (true, tester.installation_public_key().to_vec())
}

fn revoked(tester: &crate::utils::Tester) -> (bool, Vec<u8>) {
    (false, tester.installation_public_key().to_vec())
}

/// A load that stores a page and then fails reports that page. A load
/// dropped while it reads a later page reports nothing, and the next load
/// reports the page the dropped one stored.
// verifies: EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn own_installation_events_survive_a_failed_paged_load() {
    tester!(alix, disable_workers);
    tester!(alix2, from: alix, disable_workers);
    tester!(alix3, from: alix, disable_workers);
    let mut revoke = alix3
        .identity_updates()
        .revoke_installations(vec![alix2.installation_public_key().to_vec()])
        .await?;
    add_wallet_signature(&mut revoke, &alix.builder.owner).await;
    alix3
        .identity_updates()
        .apply_signature_request(revoke)
        .await?;

    // Alix holds only its own registration and reads the three later
    // updates one per page.
    let (client, script) = scripted_client(&alix, 1).await;
    let events = own_installation_events(&client);

    script.lock().reads_before_stop = Some(1);
    assert!(client.inbox_state(true).await.is_err());
    assert_eq!(drained(&events), vec![added(&alix2)]);

    let held = Arc::new(Notify::new());
    {
        let mut script = script.lock();
        script.reads_before_stop = Some(1);
        script.hold = Some(held.clone());
    }
    let load = Box::pin(client.inbox_state(true));
    let futures::future::Either::Right((_, load)) =
        futures::future::select(load, Box::pin(held.notified())).await
    else {
        panic!("the load waits on its second page");
    };
    drop(load);
    assert_eq!(drained(&events), vec![]);

    client.inbox_state(true).await?;
    assert_eq!(drained(&events), vec![added(&alix3), revoked(&alix2)]);
}

/// A publish conflict reloads the own inbox before the update is sent again.
/// A reload that stores a page and then fails still reports that page.
// verifies: EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn conflict_reload_reports_the_pages_it_stored() {
    tester!(alix, disable_workers);
    tester!(alix2, from: alix, disable_workers);
    tester!(alix3, from: alix, disable_workers);
    let (client, script) = scripted_client(&alix, xmtp_configuration::STREAM_BATCH_ROWS).await;
    let events = own_installation_events(&client);
    let mut revoke = client
        .identity_updates()
        .revoke_installations(vec![alix3.installation_public_key().to_vec()])
        .await?;
    add_wallet_signature(&mut revoke, &alix.builder.owner).await;

    {
        let mut script = script.lock();
        script.conflicts = 1;
        script.one_per_read = true;
        script.reads_before_stop = Some(1);
    }
    assert!(
        client
            .identity_updates()
            .apply_signature_request(revoke)
            .await
            .is_err()
    );
    assert!(script.lock().reads_before_stop.is_none());
    assert_eq!(drained(&events), vec![added(&alix2)]);

    script.lock().one_per_read = false;
    client.inbox_state(true).await?;
    assert_eq!(drained(&events), vec![added(&alix3)]);
}

/// A registration's publish conflict reloads the own inbox. A reload that
/// stores a page and then fails still reports that page.
// verifies: EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn registration_conflict_reload_reports_the_pages_it_stored() {
    tester!(alix, disable_workers);
    let script = Arc::new(Mutex::new(IdentityScript::default()));
    let client = ClientBuilder::new_test_builder(&alix.builder.owner)
        .await
        .api_client(ScriptedIdentity {
            inner: alix.context.api().api_client.raw_for_test().clone(),
            script: script.clone(),
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let events = own_installation_events(&client);
    // Stored after the new installation's client was built.
    tester!(alix3, from: alix, disable_workers);

    {
        let mut script = script.lock();
        script.conflicts = 1;
        script.one_per_read = true;
        script.reads_before_stop = Some(1);
    }
    assert!(
        client
            .register_with_owner(&alix.builder.owner)
            .await
            .is_err()
    );
    assert!(script.lock().reads_before_stop.is_none());
    assert_eq!(drained(&events), vec![added(&alix3)]);
}

/// An own-inbox update stored without an emission, as the incoming controller
/// stores one, before the client's first own-inbox load is reported by it.
// verifies: EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn an_update_stored_before_the_first_own_load_is_reported() {
    tester!(alix, disable_workers);
    tester!(alix2, from: alix, disable_workers);
    let (client, _) = scripted_client(&alix, xmtp_configuration::STREAM_BATCH_ROWS).await;
    let events = own_installation_events(&client);
    load_identity_updates(
        client.context.api(),
        &client.context.db(),
        &[alix.inbox_id()],
    )
    .await?;
    assert_eq!(drained(&events), vec![]);

    client.inbox_state(true).await?;
    assert_eq!(drained(&events), vec![added(&alix2)]);
}
