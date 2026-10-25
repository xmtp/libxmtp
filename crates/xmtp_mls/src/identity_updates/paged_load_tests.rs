//! Identity loads store each page before they request the next one, and a
//! load that fails after storing some pages still reports the changes to the
//! client's own installations that those pages make.

use super::*;
use crate::{builder::ClientBuilder, subscriptions::policy::StreamPolicy, tester};
use parking_lot::Mutex;
use std::sync::Arc;
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

/// Forwards every call to the test backend, except that it fails the one
/// identity read that follows `reads_before_stop` of them, as if the client
/// lost its connection there.
#[derive(Clone)]
struct StoppingIdentityReads {
    inner: crate::utils::TestClient,
    reads_before_stop: Arc<Mutex<Option<usize>>>,
}

fn is_identity_read(request: &wire::QueryRequest) -> bool {
    request.queries.iter().any(|query| {
        query.topic.as_ref().and_then(|topic| topic.topic.first())
            == Some(&(TopicKind::IdentityUpdatesV1 as u8))
    })
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for StoppingIdentityReads {
    type Error = ApiClientError;

    fn set_limits(&self, limits: Arc<xmtp_configuration::LimitsConfiguration>) {
        self.inner.set_limits(limits)
    }
    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        self.inner.publish(request).await
    }
    async fn create_upload(
        &self,
        request: wire::CreateUploadRequest,
    ) -> Result<wire::CreateUploadResponse, Self::Error> {
        self.inner.create_upload(request).await
    }
    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        if is_identity_read(&request) {
            let mut remaining = self.reads_before_stop.lock();
            match *remaining {
                Some(0) => {
                    *remaining = None;
                    return Err(stopped());
                }
                Some(reads) => *remaining = Some(reads - 1),
                None => {}
            }
        }
        self.inner.query(request).await
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

/// Whether the event adds or revokes an installation of this inbox, and which.
fn own_installation_change(event: Option<&ClientEvent>) -> (bool, Vec<u8>) {
    match event {
        Some(ClientEvent::IdentityOwnInstallationAdded(added)) => {
            (true, added.installation_key.clone())
        }
        Some(ClientEvent::IdentityOwnInstallationRevoked(revoked)) => {
            (false, revoked.installation_key.clone())
        }
        other => panic!("unexpected event {other:?}"),
    }
}

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
    // updates one per page. The first load stops after the first page.
    let reads_before_stop = Arc::new(Mutex::new(Some(1)));
    let client = ClientBuilder::from_client(alix.client.clone())
        .api_client(StoppingIdentityReads {
            inner: alix.context.api().api_client.raw_for_test().clone(),
            reads_before_stop: reads_before_stop.clone(),
        })
        .stream_policy(StreamPolicy {
            max_fetched_rows: 1,
            ..Default::default()
        })
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await?;
    let events = client.context.events().subscribe(
        EventFilter::new([
            EventKind::IdentityOwnInstallationAdded,
            EventKind::IdentityOwnInstallationRevoked,
        ]),
        Some(10),
    );
    let added = |tester: &crate::utils::Tester| (true, tester.installation_public_key().to_vec());
    let revoked =
        |tester: &crate::utils::Tester| (false, tester.installation_public_key().to_vec());
    let drain = || {
        events
            .drain()
            .iter()
            .map(|event| own_installation_change(event.client.as_ref()))
            .collect::<Vec<_>>()
    };

    assert!(client.inbox_state(true).await.is_err());
    assert!(reads_before_stop.lock().is_none());
    assert_eq!(drain(), vec![added(&alix2)]);

    client.inbox_state(true).await?;
    assert_eq!(drain(), vec![added(&alix3), revoked(&alix2)]);
}
