//! Live integration test for the v3↔d14n [`MigrationClient`] — the coexistence
//! client production actually ships (holds both a v3 and an xmtpd backend and
//! picks between them at a cutover). The rest of the suite runs on a *pure* v3
//! or *pure* d14n client selected at compile time; nothing exercises the real
//! migration client end to end. This test does: it builds an MLS client whose
//! API is the migration client, pre-migrated to the d14n side, and drives a
//! full group-message round-trip through it against the live xmtpd backend.
//!
//! Two things are exercised here, both with the client parked on one side of
//! the cutover (so neither needs the transition itself):
//!   - the d14n (post-cutover) side, via a full group-message round-trip; and
//!   - commit-log routing, which must keep going to the retained v3 service even
//!     after migration — proven by a live publish→query round-trip that only
//!     round-trips against node-go, never xmtpd's commit-log no-op.
//!
//! The cutover *transition* isn't driven: the pre-cutover selection is covered
//! by the migration unit tests in `xmtp_api_d14n::queries::combined::tests`, and
//! it can't run against the local stack because the test node-go does not serve
//! `FetchD14nCutover`, so `choose_client`'s forced first refresh would error
//! before it could route to v3. See `ClientBuilder::local_migration`.
//!
//! d14n-only and native-only.
#![allow(clippy::unwrap_used)]

use alloy::signers::local::PrivateKeySigner;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;

use crate::Client;
use crate::context::XmtpSharedContext;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::utils::test::{MigrationXmtpClient, identity_setup, register_client};

/// Build a registered MLS client whose API is the migration client, pre-set to
/// the migrated (d14n) side so every call routes to xmtpd.
async fn migrated_migration_client(owner: &PrivateKeySigner) -> MigrationXmtpClient {
    let client = Client::builder(identity_setup(owner.clone()))
        .temp_store()
        .await
        .local_migration()
        .await
        .default_mls_store()
        .unwrap()
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .build()
        .await
        .unwrap();
    register_client(&client, owner.clone()).await;
    client
}

/// Two migration clients, both migrated to d14n, exchange a group message end to
/// end — proving the real cutover client registers, publishes, and reads over
/// the live xmtpd backend.
#[xmtp_common::test(unwrap_try = true)]
async fn migration_client_delivers_a_group_message_on_the_d14n_backend() {
    let alix = migrated_migration_client(&generate_local_wallet()).await;
    let bo = migrated_migration_client(&generate_local_wallet()).await;

    let group = alix.create_group(None, None).await?;
    group.add_members(&[bo.inbox_id()]).await?;

    const MSG: &[u8] = b"hello over d14n through the migration client";
    group.send_message(MSG, SendMessageOpts::default()).await?;

    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id).await?;
    let last = bo_group.test_last_message_bytes().await?;
    assert_eq!(last.unwrap(), MSG);
}

/// A migrated client sends every ordinary call to xmtpd — but commit-log (fork
/// detection) must keep going to the retained v3 service, never to xmtpd (whose
/// commit-log methods are deliberate no-ops). This drives a real publish→query
/// commit-log round-trip through a *migrated* migration client and asserts the
/// entry comes back: had commit-log wrongly followed the cutover to xmtpd, the
/// publish would hit the d14n no-op and the query would return empty. It's the
/// live counterpart to the `commit_log_stays_on_v3_after_migration` mock unit
/// test in `xmtp_api_d14n::queries::combined::tests`.
#[xmtp_common::test(unwrap_try = true)]
async fn migration_client_commit_log_round_trips_through_v3() {
    use openmls::prelude::{OpenMlsCrypto, SignatureScheme};
    use openmls_traits::OpenMlsProvider;
    use prost::Message;
    use rand::RngExt;
    use xmtp_proto::mls_v1::{PublishCommitLogRequest, QueryCommitLogRequest};
    use xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature;
    use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

    let alix = migrated_migration_client(&generate_local_wallet()).await;

    // The local node's commit log isn't cleared between runs; use a fresh random
    // group_id so this test never collides with a previous iteration.
    let group_id: Vec<u8> = (0..20).map(|_| rand::rng().random_range(0..=255)).collect();

    let entry = PlaintextCommitLogEntry {
        group_id: group_id.clone(),
        commit_sequence_id: 1,
        last_epoch_authenticator: vec![1, 2, 3, 4],
        commit_result: 1, // Success
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![5, 6, 7, 8],
    };

    // The backend requires a signature; sign the serialized entry with an ad-hoc
    // ed25519 key (unrelated to the client's identity, which lives on xmtpd).
    let provider = alix.context.mls_provider();
    let crypto = provider.crypto();
    let (private_key_bytes, _) = crypto.signature_key_gen(SignatureScheme::ED25519)?;
    let private_key = xmtp_cryptography::Secret::new(private_key_bytes.clone());
    let public_key = xmtp_cryptography::signature::to_public_key(&private_key)?.to_vec();
    let serialized = entry.encode_to_vec();
    let signature = crypto.sign(SignatureScheme::ED25519, &serialized, &private_key_bytes)?;

    alix.context
        .api()
        .publish_commit_log(vec![PublishCommitLogRequest {
            group_id: group_id.clone(),
            serialized_commit_log_entry: serialized,
            signature: Some(RecoverableEd25519Signature {
                bytes: signature,
                public_key,
            }),
        }])
        .await?;

    let responses = alix
        .context
        .api()
        .query_commit_log(vec![QueryCommitLogRequest {
            group_id: group_id.clone(),
            ..Default::default()
        }])
        .await?;

    // A non-empty result means the round-trip landed on live node-go (v3); a
    // client that routed commit-log to xmtpd would see the no-op and get nothing.
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].commit_log_entries.len(), 1);
    let returned = PlaintextCommitLogEntry::decode(
        responses[0].commit_log_entries[0]
            .serialized_commit_log_entry
            .as_slice(),
    )?;
    assert_eq!(returned.group_id, group_id);
    assert_eq!(returned.commit_sequence_id, 1);
}
