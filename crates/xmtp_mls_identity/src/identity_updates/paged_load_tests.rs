//! Identity loads store each page before they request the next one, and a
//! load that fails after storing some pages still reports the changes to the
//! client's own installations that those pages make.

use super::test_support::{IdentityScript, ScriptedIdentity, stopped};
use super::*;
use crate::identity_lifecycle::test_support::{IdentityTestFeatures, IdentityTestPeer, TestPeer};
use parking_lot::Mutex;
use std::sync::Arc;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::XmtpTestDb;
use xmtp_id::associations::test_utils::{WalletTestExt, add_wallet_signature};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_mls_core::policy::StreamPolicy;
use xmtp_proto::{
    api::ApiClientError, backend_v1 as wire, types::Topic,
    xmtp::identity::associations::IdentityUpdate,
};

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
    mock.expect_query_raw_envelopes()
        .times(1)
        .in_sequence(&mut sequence)
        .returning(move |request| {
            assert_eq!(queried_after(&request), 0);
            assert_eq!(request.limit, xmtp_configuration::STREAM_BATCH_ROWS);
            Ok(wire::QueryResponse {
                envelopes: first_page.clone(),
                continuation: Some(wire::Continuation { has_more: true }),
            }
            .into())
        });
    mock.expect_query_raw_envelopes()
        .times(1)
        .in_sequence(&mut sequence)
        .returning(move |request| {
            assert_eq!(queried_after(&request), 2);
            Err(stopped())
        });
    mock.expect_query_raw_envelopes()
        .times(1)
        .in_sequence(&mut sequence)
        .returning(move |request| {
            assert_eq!(queried_after(&request), 2);
            Ok(wire::QueryResponse {
                envelopes: last_page.clone(),
                continuation: Some(wire::Continuation { has_more: false }),
            }
            .into())
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

type ScriptedPeer = TestPeer<IdentityTestFeatures, ScriptedIdentity>;

/// An installation of `base`'s inbox and database whose backend calls go
/// through a script, reading at most `max_fetched_rows` envelopes per page.
async fn scripted_client(
    base: &IdentityTestPeer,
    max_fetched_rows: u32,
) -> (ScriptedPeer, Arc<Mutex<IdentityScript>>) {
    scripted_client_verifying(base, max_fetched_rows, None).await
}

/// As [`scripted_client`], verifying smart contract wallets with `verifier`
/// when one is given.
async fn scripted_client_verifying(
    base: &IdentityTestPeer,
    max_fetched_rows: u32,
    verifier: Option<Arc<RateLimitedVerifier>>,
) -> (ScriptedPeer, Arc<Mutex<IdentityScript>>) {
    let script = Arc::new(Mutex::new(IdentityScript::default()));
    let client = base
        .with_api(
            ScriptedIdentity {
                inner: base.context.api().api_client.raw_for_test().clone(),
                script: script.clone(),
            },
            verifier.map(|verifier| Box::new(verifier) as Box<dyn SmartContractSignatureVerifier>),
            |options| {
                options.stream_policy = StreamPolicy {
                    max_fetched_rows,
                    ..Default::default()
                }
            },
        )
        .await;
    (client, script)
}

/// A rate-limited batched proof fetch returns its rejection: the inboxes are
/// not then fetched one request each.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_batched_proof_fetch_is_not_split() {
    use crate::identity_updates::dependencies::{
        IdentityRequirement, resolve_identity_requirements,
    };
    let alix = IdentityTestPeer::new().await;
    let bo = IdentityTestPeer::new().await;
    let caro = IdentityTestPeer::new().await;
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

/// Refuses every smart contract wallet check with a rate limit, and counts
/// the checks. Each refusal's delay is 300 s; with `growing`, the n-th check's
/// is n × 300 s. It answers without waiting, so refusals finish in call order
/// and the first to finish has the shortest delay.
#[derive(Default)]
struct RateLimitedVerifier {
    calls: std::sync::atomic::AtomicUsize,
    growing: bool,
}

#[xmtp_common::async_trait]
impl xmtp_id::scw_verifier::SmartContractSignatureVerifier for RateLimitedVerifier {
    async fn is_valid_signature(
        &self,
        _: xmtp_id::associations::AccountId,
        _: [u8; 32],
        _: alloy::primitives::Bytes,
        _: Option<alloy::primitives::BlockNumber>,
    ) -> Result<xmtp_id::scw_verifier::ValidationResponse, xmtp_id::scw_verifier::VerifierError>
    {
        let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) as u64;
        let steps = if self.growing { call + 1 } else { 1 };
        Err(xmtp_id::scw_verifier::VerifierError::Other(Box::new(
            ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
                xmtp_api_grpc::error::rate_limited_status(
                    "chain_verify",
                    xmtp_common::time::Duration::from_secs(300 * steps),
                ),
            )),
        )))
    }
}

/// An inbox created by a smart contract wallet, as an envelope at `sequence_id`.
fn scw_inbox(sequence_id: u64) -> (String, wire::ServerEnvelope) {
    use xmtp_id::associations::{
        AccountId, Identifier,
        unsigned_actions::UnsignedCreateInbox,
        unverified::{
            UnverifiedAction, UnverifiedCreateInbox, UnverifiedIdentityUpdate, UnverifiedSignature,
        },
    };
    let wallet = Identifier::rand_ethereum();
    let inbox_id = wallet.inbox_id(0).unwrap();
    let create = UnverifiedAction::CreateInbox(UnverifiedCreateInbox::new(
        UnsignedCreateInbox {
            nonce: 0,
            account_identifier: wallet.clone(),
        },
        UnverifiedSignature::new_smart_contract_wallet(
            vec![1],
            AccountId::new_evm(1, wallet.to_string()),
            1,
        ),
    ));
    let update = UnverifiedIdentityUpdate::new(inbox_id.clone(), 1, vec![create]).into();
    (inbox_id, identity_envelope(&update, sequence_id))
}

/// A verification refused with a rate limit while the batched fetch verifies
/// its proofs is returned at once, not verified again in a later round.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_prefetched_verification_is_not_sent_again() {
    use crate::identity_updates::dependencies::{
        IdentityRequirement, resolve_identity_requirements,
    };
    let alix = IdentityTestPeer::new().await;
    let verifier = Arc::new(RateLimitedVerifier::default());
    let (client, script) = scripted_client_verifying(&alix, 100, Some(verifier.clone())).await;
    let (x, x_envelope) = scw_inbox(1);
    let (y, y_envelope) = scw_inbox(2);
    script.lock().served = Some(vec![x_envelope, y_envelope]);
    let requirements = [(x, 1), (y, 2)]
        .into_iter()
        .map(|(inbox_id, sequence_id)| IdentityRequirement {
            inbox_id,
            sequence_id,
        })
        .collect();
    let error = resolve_identity_requirements(&client.context, &requirements)
        .await
        .unwrap_err();
    assert_eq!(
        error.rate_limit_delay(),
        Some(xmtp_common::time::Duration::from_secs(300))
    );
    assert_eq!(
        verifier.calls.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "each refused check is sent once"
    );
    assert_eq!(script.lock().reads, 1, "no per-inbox reads");
}

/// Of several refused verifications in one batched fetch, the longest delay is
/// returned, so the caller does not retry a proof inside its own delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_batched_fetch_returns_its_longest_refused_delay() {
    use crate::identity_updates::dependencies::{
        IdentityRequirement, resolve_identity_requirements,
    };
    let alix = IdentityTestPeer::new().await;
    let verifier = Arc::new(RateLimitedVerifier {
        growing: true,
        ..Default::default()
    });
    let (client, script) = scripted_client_verifying(&alix, 100, Some(verifier.clone())).await;
    let (x, x_envelope) = scw_inbox(1);
    let (y, y_envelope) = scw_inbox(2);
    script.lock().served = Some(vec![x_envelope, y_envelope]);
    let requirements = [(x, 1), (y, 2)]
        .into_iter()
        .map(|(inbox_id, sequence_id)| IdentityRequirement {
            inbox_id,
            sequence_id,
        })
        .collect();
    let error = resolve_identity_requirements(&client.context, &requirements)
        .await
        .unwrap_err();
    assert_eq!(
        error.rate_limit_delay(),
        Some(xmtp_common::time::Duration::from_secs(600))
    );
}
