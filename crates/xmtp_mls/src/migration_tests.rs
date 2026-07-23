//! Live integration test for the v3↔d14n [`MigrationClient`] — the coexistence
//! client production actually ships (holds both a v3 and an xmtpd backend and
//! picks between them at a cutover). The rest of the suite runs on a *pure* v3
//! or *pure* d14n client selected at compile time; nothing exercises the real
//! migration client end to end. This test does: it builds an MLS client whose
//! API is the migration client, pre-migrated to the d14n side, and drives a
//! full group-message round-trip through it against the live xmtpd backend.
//!
//! Only the d14n (post-cutover) side is driven here. The v3 (pre-cutover)
//! selection is covered by the migration unit tests in
//! `xmtp_api_d14n::queries::combined::tests`; it can't be driven against the
//! local stack because the test node-go does not serve `FetchD14nCutover`, so
//! `choose_client`'s forced first refresh would error before it could route to
//! v3. See `ClientBuilder::local_migration`.
//!
//! d14n-only and native-only.
#![allow(clippy::unwrap_used)]

use alloy::signers::local::PrivateKeySigner;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;

use crate::Client;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::utils::test::{MigrationXmtpClient, identity_setup, register_client};

/// Build a registered MLS client whose API is the migration client, pre-set to
/// the migrated (d14n) side so every call routes to xmtpd.
async fn migrated_migration_client(owner: &PrivateKeySigner) -> MigrationXmtpClient {
    let client = Client::builder(identity_setup(owner.clone()))
        .temp_store()
        .await
        .local_migration()
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

    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;

    const MSG: &[u8] = b"hello over d14n through the migration client";
    group.send_message(MSG, SendMessageOpts::default()).await?;

    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let last = bo_group.test_last_message_bytes().await?;
    assert_eq!(last.unwrap(), MSG);
}
