//! Identity loads store each page before they request the next one, and a
//! load that fails after storing some pages still reports the changes to the
//! client's own installations that those pages make.

use super::*;
use crate::{builder::ClientBuilder, subscriptions::policy::StreamPolicy, tester};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::Notify;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::XmtpTestDb;
use xmtp_events::{ClientEvent, EventFilter, EventKind};
use xmtp_id::associations::test_utils::{WalletTestExt, add_wallet_signature};
use xmtp_proto::{
    api::ApiClientError,
    backend_v1 as wire,
    types::{Topic, TopicKind},
    xmtp::identity::associations::IdentityUpdate,
};

fn stopped() -> ApiClientError {
    ApiClientError::OtherUnretryable(Box::new(tonic::Status::new(
        tonic::Code::Unavailable,
        "stopped by the test",
    )))
}

fn identity_envelope(update: &IdentityUpdate, sequence_id: u64) -> wire::ServerEnvelope {
    wire::ServerEnvelope {
        meta: Some(wire::EnvelopeMeta {
            topic: Some(wire::Topic {
                topic: Topic::new_identity_update(hex::decode(&update.inbox_id).unwrap())
                    .cloned_vec(),
            }),
            cursor: Some(wire::Cursor { sequence_id }),
            server_ns: 1,
            message_hash: Some(wire::MessageHash {
                hash: Some(wire::message_hash::Hash::Sha256(vec![
                    sequence_id as u8;
                    32
                ])),
            }),
            ..Default::default()
        }),
        envelope: Some(wire::ClientEnvelope {
            payload: Some(wire::client_envelope::Payload::IdentityUpdate(
                update.clone(),
            )),
        }),
    }
}

/// An inbox's creation followed by two recovery address changes.
async fn three_updates() -> Vec<IdentityUpdate> {
    let mut wallet = generate_local_wallet();
    let inbox_id = wallet.identifier().inbox_id(0).unwrap();
    let mut request = SignatureRequestBuilder::new(&inbox_id)
        .create_inbox(wallet.identifier(), 0)
        .build();
    add_wallet_signature(&mut request, &wallet).await;
    let mut updates = vec![request.build_identity_update().unwrap().into()];
    for _ in 0..2 {
        let next = generate_local_wallet();
        let mut request = SignatureRequestBuilder::new(&inbox_id)
            .change_recovery_address(wallet.identifier().into(), next.identifier())
            .build();
        add_wallet_signature(&mut request, &wallet).await;
        updates.push(request.build_identity_update().unwrap().into());
        wallet = next;
    }
    updates
}

#[xmtp_common::test(unwrap_try = true)]
async fn load_identity_updates_stores_each_page() {
    let updates = three_updates().await;
    let inbox_id = updates[0].inbox_id.clone();
    let queried_after = |request: &wire::QueryRequest| {
        assert_eq!(request.queries.len(), 1);
        request.queries[0].cursor.as_ref().unwrap().sequence_id
    };
    let first_page = vec![
        identity_envelope(&updates[0], 1),
        identity_envelope(&updates[1], 2),
    ];
    let last_page = vec![identity_envelope(&updates[2], 3)];
    let mut sequence = mockall::Sequence::new();
    let mut mock = xmtp_api_backend::MockBackendClient::new();
    mock.expect_query()
        .times(1)
        .in_sequence(&mut sequence)
        .returning(move |request| {
            assert_eq!(queried_after(&request), 0);
            assert_eq!(request.limit, xmtp_configuration::STREAM_BATCH_ROWS);
            Ok(wire::QueryResponse {
                envelopes: first_page.clone(),
                continuation: Some(wire::Continuation { has_more: true }),
            })
        });
    mock.expect_query()
        .times(1)
        .in_sequence(&mut sequence)
        .returning(move |request| {
            assert_eq!(queried_after(&request), 2);
            Err(stopped())
        });
    mock.expect_query()
        .times(1)
        .in_sequence(&mut sequence)
        .returning(move |request| {
            assert_eq!(queried_after(&request), 2);
            Ok(wire::QueryResponse {
                envelopes: last_page.clone(),
                continuation: Some(wire::Continuation { has_more: false }),
            })
        });
    let api = ApiClientWrapper::new(mock, Retry::default());
    let store = xmtp_db::TestDb::create_ephemeral_store().await;
    let db = store.db();
    let stored = || {
        db.get_identity_updates(&inbox_id, None, None)
            .unwrap()
            .iter()
            .map(|update| update.sequence_id)
            .collect::<Vec<_>>()
    };

    assert!(
        load_identity_updates(&api, &db, &[inbox_id.as_str()])
            .await
            .is_err()
    );
    assert_eq!(stored(), vec![1, 2]);
    load_identity_updates(&api, &db, &[inbox_id.as_str()]).await?;
    assert_eq!(stored(), vec![1, 2, 3]);
}

/// What the test backend does to identity publishes and reads.
#[derive(Default)]
struct IdentityScript {
    /// Identity publishes still to refuse as a commit-time conflict.
    conflicts: usize,
    /// Answer each identity read with at most one update, and more to follow.
    one_per_read: bool,
    /// Identity reads still answered before the next one stops.
    reads_before_stop: Option<usize>,
    /// Wake this and hold the stopped read until the caller drops it, instead
    /// of failing the read.
    hold: Option<Arc<Notify>>,
    /// Refuse every identity read with a rate limit of this delay.
    rate_limit: Option<xmtp_common::time::Duration>,
    /// Identity reads received.
    reads: usize,
}

/// Forwards every call to the test backend, changing identity publishes and
/// reads as its script says.
#[derive(Clone)]
struct ScriptedIdentity {
    inner: crate::utils::TestClient,
    script: Arc<Mutex<IdentityScript>>,
}

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

fn is_identity_read(request: &wire::QueryRequest) -> bool {
    request.queries.iter().any(|query| {
        query.topic.as_ref().and_then(|topic| topic.topic.first())
            == Some(&(TopicKind::IdentityUpdatesV1 as u8))
    })
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for ScriptedIdentity {
    type Error = ApiClientError;

    fn set_limits(&self, limits: Arc<xmtp_configuration::LimitsConfiguration>) {
        self.inner.set_limits(limits)
    }
    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        let identity = request.envelopes.iter().any(|envelope| {
            matches!(
                envelope.payload,
                Some(wire::client_envelope::Payload::IdentityUpdate(_))
            )
        });
        let conflict = identity && {
            let mut script = self.script.lock();
            let conflict = script.conflicts > 0;
            script.conflicts = script.conflicts.saturating_sub(1);
            conflict
        };
        if conflict {
            return Err(ApiClientError::client(
                xmtp_api_grpc::error::GrpcError::Status(tonic::Status::aborted(
                    "identity conflict",
                )),
            ));
        }
        self.inner.publish(request).await
    }
    async fn create_upload(
        &self,
        request: wire::CreateUploadRequest,
    ) -> Result<wire::CreateUploadResponse, Self::Error> {
        self.inner.create_upload(request).await
    }
    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        if !is_identity_read(&request) {
            return self.inner.query(request).await;
        }
        let rate_limit = {
            let mut script = self.script.lock();
            script.reads += 1;
            script.rate_limit
        };
        if let Some(delay) = rate_limit {
            return Err(ApiClientError::client(
                xmtp_api_grpc::error::GrpcError::Status(xmtp_api_grpc::error::rate_limited_status(
                    "request", delay,
                )),
            ));
        }
        let (stop, hold, one_per_read) = {
            let mut script = self.script.lock();
            let stop = script.reads_before_stop == Some(0);
            script.reads_before_stop = script
                .reads_before_stop
                .and_then(|reads| reads.checked_sub(1));
            (stop, script.hold.clone(), script.one_per_read)
        };
        if stop {
            if let Some(hold) = hold {
                hold.notify_one();
                return std::future::pending().await;
            }
            return Err(stopped());
        }
        let mut response = self.inner.query(request).await?;
        if one_per_read && response.envelopes.len() > 1 {
            response.envelopes.truncate(1);
            response.continuation = Some(wire::Continuation { has_more: true });
        }
        Ok(response)
    }
    async fn query_newest(
        &self,
        request: wire::QueryNewestRequest,
    ) -> Result<wire::QueryNewestResponse, Self::Error> {
        self.inner.query_newest(request).await
    }
    async fn get_inbox_ids(
        &self,
        request: wire::GetInboxIdsRequest,
    ) -> Result<wire::GetInboxIdsResponse, Self::Error> {
        self.inner.get_inbox_ids(request).await
    }
    async fn get_configuration(
        &self,
        request: wire::GetConfigurationRequest,
    ) -> Result<wire::GetConfigurationResponse, Self::Error> {
        self.inner.get_configuration(request).await
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: wire::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<wire::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.inner
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
    async fn register(
        &self,
        request: wire::RegisterRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.inner.register(request).await
    }
    async fn unregister(
        &self,
        request: wire::UnregisterRequest,
    ) -> Result<wire::UnregisterResponse, Self::Error> {
        self.inner.unregister(request).await
    }
    async fn update_subscriptions(
        &self,
        request: wire::UpdateSubscriptionsRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.inner.update_subscriptions(request).await
    }
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

/// A rate-limited batched proof fetch returns its rejection: the inboxes are
/// not then fetched one request each.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_batched_proof_fetch_is_not_split() {
    use crate::identity_updates::dependencies::{
        IdentityRequirement, resolve_identity_requirements,
    };
    tester!(alix, disable_workers);
    tester!(bo);
    tester!(caro);
    let (client, script) = scripted_client(&alix, 100).await;
    let delay = xmtp_common::time::Duration::from_secs(300);
    script.lock().rate_limit = Some(delay);
    let requirements = [&bo, &caro]
        .into_iter()
        .map(|tester| IdentityRequirement {
            inbox_id: tester.inbox_id().to_string(),
            sequence_id: 1,
        })
        .collect();
    let error = resolve_identity_requirements(&client.context, &requirements)
        .await
        .unwrap_err();
    assert_eq!(error.rate_limit_delay(), Some(delay));
    assert_eq!(
        script.lock().reads,
        1,
        "one batched read, no per-inbox reads"
    );
}
