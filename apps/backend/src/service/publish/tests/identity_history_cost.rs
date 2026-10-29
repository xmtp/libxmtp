//! What an identity publish costs in history reads and chain calls.

use super::*;
use crate::service::publish::MAX_IDENTITY_REPLAYS;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering::SeqCst},
};
use xmtp_configuration::{
    MAX_IDENTITY_UPDATE_ACTIONS, MAX_PUBLISH_IDENTITY_UPDATES, MAX_SCW_SIGNATURE_BYTES,
};
use xmtp_id::associations::{
    AccountId, MemberIdentifier,
    builder::SignatureRequestBuilder,
    test_utils::{WalletTestExt, add_wallet_signature},
    unsigned_actions::{UnsignedClaimHandle, UnsignedRevokeAssociation},
    unverified::{
        UnverifiedAction, UnverifiedClaimHandle, UnverifiedIdentityUpdate,
        UnverifiedRevokeAssociation, UnverifiedSignature,
    },
};
use xmtp_id::scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError};
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;

/// The smart contract wallet that creates the fixture inbox, and so holds its
/// recovery role.
const WALLET: &str = "0x1111111111111111111111111111111111111111";

/// The SCW revocations in the fixture log after its creation.
const HISTORY_REVOCATIONS: u8 = 100;

/// The verifier calls that admitting the fixture log costs: the creation and
/// each revocation.
const HISTORY_CHECKS: usize = HISTORY_REVOCATIONS as usize + 1;

/// The ERC-6492 signature bytes that the fixture verifier never answers for.
const UNANSWERED: [u8; 2] = [0xfd, 0];

/// Model the provider work behind the backend's bounded SCW verdict cache.
/// It never answers for an [`UNANSWERED`] signature.
struct CountingVerifier(Arc<AtomicUsize>);

#[xmtp_common::async_trait]
impl SmartContractSignatureVerifier for CountingVerifier {
    async fn is_valid_signature(
        &self,
        _account_id: AccountId,
        _hash: [u8; 32],
        signature: alloy_primitives::Bytes,
        block_number: Option<u64>,
    ) -> Result<ValidationResponse, VerifierError> {
        self.0.fetch_add(1, SeqCst);
        if signature.as_ref() == UNANSWERED {
            std::future::pending::<()>().await;
        }
        Ok(ValidationResponse {
            is_valid: true,
            block_number,
            error: None,
        })
    }
}

/// An ERC-6492 signature of the fixture wallet at block 1.
fn scw(bytes: Vec<u8>) -> UnverifiedSignature {
    UnverifiedSignature::new_smart_contract_wallet(bytes, AccountId::new_evm(1, WALLET.into()), 1)
}

/// A revocation of an identifier the inbox does not hold, which applies.
fn revoke(signature: UnverifiedSignature) -> UnverifiedAction {
    UnverifiedAction::RevokeAssociation(UnverifiedRevokeAssociation::new(
        UnsignedRevokeAssociation {
            revoked_member: MemberIdentifier::eth("0x2222222222222222222222222222222222222222")
                .expect("the fixture identifier is valid"),
        },
        signature,
    ))
}

fn update(
    inbox_id: &str,
    client_timestamp_ns: u64,
    actions: Vec<UnverifiedAction>,
) -> IdentityUpdate {
    UnverifiedIdentityUpdate::new(inbox_id.into(), client_timestamp_ns, actions).into()
}

/// A server whose verdict cache holds one entry, and a stored log of two
/// updates for the fixture wallet's inbox that carry [`HISTORY_CHECKS`]
/// distinct SCW signatures, more than the cache holds, so verifying the log
/// again would cost one verifier call for each. Returns the verifier's call
/// count and the inbox id.
async fn scw_history() -> support::TestResult<(TestServer, Arc<AtomicUsize>, String)> {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = TestServer::with_verifier(
        |config| config.validation.max_scw_cache_entries = 1,
        CountingVerifier(calls.clone()),
        TestChain::at(1),
    )
    .await?;
    let create = scw_create_inbox_update_at(1);
    let inbox_id = create.inbox_id.clone();
    assert_eq!(publish_identity(&server, create).await?, (Ok(()), 1));
    let revocations = (0..HISTORY_REVOCATIONS)
        .map(|n| revoke(scw(vec![0xff, n])))
        .collect();
    let revocations = update(&inbox_id, 2, revocations);
    assert_eq!(publish_identity(&server, revocations).await?, (Ok(()), 2));
    assert_eq!(calls.load(SeqCst), HISTORY_CHECKS);
    Ok((server, calls, inbox_id))
}

/// Create the inbox of a new wallet and return an update for it, signed by
/// the wallet, that revokes an identifier the inbox does not hold.
async fn wallet_inbox_update(server: &TestServer) -> support::TestResult<api::ClientEnvelope> {
    let wallet = xmtp_cryptography::utils::generate_local_wallet();
    let inbox_id = wallet.get_inbox_id(0);
    let mut create = SignatureRequestBuilder::new(&inbox_id)
        .create_inbox(wallet.identifier(), 0)
        .build();
    add_wallet_signature(&mut create, &wallet).await;
    server
        .publish(vec![identity_envelope(
            create.build_identity_update()?.into(),
        )])
        .await?;
    let mut update = SignatureRequestBuilder::new(&inbox_id)
        .revoke_association(
            wallet.member_identifier(),
            MemberIdentifier::eth("0x2222222222222222222222222222222222222222")?,
        )
        .build();
    add_wallet_signature(&mut update, &wallet).await;
    Ok(identity_envelope(update.build_identity_update()?.into()))
}

/// Set every stored row's payload to NULL, which a history read cannot
/// decode, so any read of a log fails as `UNAVAILABLE`.
async fn make_logs_unreadable(server: &TestServer) -> support::TestResult {
    sqlx::raw_sql(
        "ALTER TABLE envelopes ALTER COLUMN payload DROP NOT NULL;
         UPDATE envelopes SET payload = NULL;",
    )
    .execute(&server.backend.store.primary)
    .await?;
    Ok(())
}

/// A valid update costs a verifier call for each of its own SCW signatures and
/// none for the stored log's, which the backend verified when it admitted
/// them, however many more verdicts the log holds than the cache.
// verifies: IDENT-095
#[xmtp_common::test(unwrap_try = true)]
async fn valid_identity_update_verifies_only_its_own_scw_signatures() {
    let (server, calls, inbox_id) = scw_history().await?;
    let own = (0..3).map(|n| revoke(scw(vec![0xfe, n]))).collect();
    assert_eq!(
        publish_identity(&server, update(&inbox_id, 3, own)).await?,
        (Ok(()), 3)
    );
    assert_eq!(calls.load(SeqCst), HISTORY_CHECKS + 3);
    server.stop().await?;
}

/// Admission replays an inbox's stored log in a blocking task, never on the
/// thread that runs the request, once for each update it validates against
/// that log. Publishing the fixture log replays it once, for the revocations;
/// the creation has no log to replay.
#[xmtp_common::test(unwrap_try = true)]
async fn identity_history_replay_runs_off_the_request_thread() {
    let (server, _, _) = scw_history().await?;
    assert_eq!(server.backend.identity_replays.load(SeqCst), 1);
    server.stop().await?;
}

/// Admission that outlasts its deadline fails as `DEADLINE_EXCEEDED` and
/// stores nothing, whether it waits for a replay permit or for a verifier.
/// Only identity updates take a permit, so a group message still publishes
/// while every permit is held, and the update stores once they are free.
#[xmtp_common::timeout(std::time::Duration::from_secs(60))]
#[xmtp_common::test(unwrap_try = true)]
async fn admission_past_its_deadline_stores_nothing() {
    let (server, _, inbox_id) = scw_history().await?;
    let backend = &server.backend;
    *backend.admission_deadline.lock() = Some(std::time::Duration::from_secs(2));
    let next = || update(&inbox_id, 3, vec![revoke(scw(vec![0xfe, 0]))]);
    let held = backend
        .replays
        .clone()
        .acquire_many_owned(MAX_IDENTITY_REPLAYS as u32)
        .await?;
    assert_eq!(
        publish_identity(&server, next()).await?,
        (Err((Code::DeadlineExceeded, None)), 2)
    );
    assert_eq!(backend.identity_replays.load(SeqCst), 1);
    let message = keyed_group_message_envelope(&group_keys(1), GroupMessageKind::Application, []);
    server.publish(vec![message]).await?;
    drop(held);

    let unanswered = update(&inbox_id, 3, vec![revoke(scw(UNANSWERED.into()))]);
    assert_eq!(
        publish_identity(&server, unanswered).await?,
        (Err((Code::DeadlineExceeded, None)), 3)
    );
    assert_eq!(publish_identity(&server, next()).await?, (Ok(()), 4));
    server.stop().await?;
}

/// An update waits for a replay permit before it reads its inbox's log, so
/// while every permit is held it times out without a read. The log is
/// unreadable, so a read would fail as `UNAVAILABLE` instead.
// verifies: IDENT-095
#[xmtp_common::timeout(std::time::Duration::from_secs(60))]
#[xmtp_common::test(unwrap_try = true)]
async fn replay_slot_is_taken_before_the_log_is_read() {
    let (server, _, inbox_id) = scw_history().await?;
    let backend = &server.backend;
    *backend.admission_deadline.lock() = Some(std::time::Duration::from_secs(2));
    let _held = backend
        .replays
        .clone()
        .acquire_many_owned(MAX_IDENTITY_REPLAYS as u32)
        .await?;
    make_logs_unreadable(&server).await?;
    let next = update(&inbox_id, 3, vec![revoke(scw(vec![0xfe, 0]))]);
    assert_eq!(
        publish_identity(&server, next).await?,
        (Err((Code::DeadlineExceeded, None)), 2)
    );
    server.stop().await?;
}

/// A publish carries at most [`MAX_PUBLISH_IDENTITY_UPDATES`] identity
/// updates, each for its own inbox. One more fails the whole request as too
/// large before any inbox's log is replayed, and stores nothing.
#[xmtp_common::test(unwrap_try = true)]
async fn publish_identity_update_limit_accepts_exact_count_and_rejects_one_past() {
    let server = TestServer::new(|_| {}).await?;
    let mut updates = Vec::new();
    for _ in 0..=MAX_PUBLISH_IDENTITY_UPDATES {
        updates.push(wallet_inbox_update(&server).await?);
    }
    let error = server
        .publisher()
        .publish(api::PublishRequest {
            envelopes: updates.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), Code::InvalidArgument);
    let status = tonic_types::pb::Status::decode(error.details())?;
    let detail = api::PublishError::decode(status.details[0].value.as_slice())?;
    assert_eq!((detail.index, detail.reason()), (None, Reason::TooLarge));
    assert_eq!(server.backend.identity_replays.load(SeqCst), 0);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(rows, updates.len() as i64);

    updates.pop();
    assert_eq!(
        server.publish(updates).await?.len(),
        MAX_PUBLISH_IDENTITY_UPDATES
    );
    assert_eq!(
        server.backend.identity_replays.load(SeqCst),
        MAX_PUBLISH_IDENTITY_UPDATES
    );
    server.stop().await?;
}

/// Each update that a check needing no history rejects fails for its own
/// reason without reading its inbox's log. The log is unreadable, so any read
/// of it fails as `UNAVAILABLE`, as it does for the well-formed control. None
/// fails as `ABORTED`, although the inbox has a head that the update never
/// read.
// verifies: API-302, API-303
#[xmtp_common::test(unwrap_try = true)]
async fn identity_shape_rejections_read_no_history() {
    const MAX_SCW_SIGNATURES: u8 = 2;
    let server = TestServer::with_verifier(
        |config| config.limits.max_scw_signatures = MAX_SCW_SIGNATURES.into(),
        CountingVerifier(Arc::default()),
        TestChain::at(1),
    )
    .await?;
    let create = scw_create_inbox_update_at(1);
    let inbox_id = create.inbox_id.clone();
    assert_eq!(publish_identity(&server, create).await?, (Ok(()), 1));
    make_logs_unreadable(&server).await?;

    let ecdsa = || UnverifiedSignature::new_recoverable_ecdsa(vec![1; 65]);
    let claim = UnverifiedAction::ClaimHandle(UnverifiedClaimHandle::new(
        UnsignedClaimHandle {
            handle: "nick".into(),
        },
        ecdsa(),
    ));
    let invalid = Err((Code::InvalidArgument, Some(Reason::InvalidIdentityUpdate)));
    for (case, actions, result) in [
        ("no actions", vec![], invalid),
        (
            "a claim beside another action",
            vec![revoke(ecdsa()), claim],
            invalid,
        ),
        (
            "101 actions",
            (0..=MAX_IDENTITY_UPDATE_ACTIONS)
                .map(|_| revoke(ecdsa()))
                .collect(),
            invalid,
        ),
        (
            "an oversized ERC-6492 signature",
            vec![revoke(scw(vec![1; MAX_SCW_SIGNATURE_BYTES + 1]))],
            Err((Code::InvalidArgument, Some(Reason::InvalidSignature))),
        ),
        (
            "too many ERC-6492 signatures",
            (0..=MAX_SCW_SIGNATURES)
                .map(|n| revoke(scw(vec![n])))
                .collect(),
            Err((Code::InvalidArgument, Some(Reason::TooLarge))),
        ),
        (
            "the well-formed control",
            vec![revoke(ecdsa())],
            Err((Code::Unavailable, None)),
        ),
    ] {
        assert_eq!(
            publish_identity(&server, update(&inbox_id, 2, actions)).await?,
            (result, 1),
            "{case}"
        );
    }
    server.stop().await?;
}
