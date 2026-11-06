//! One inbox per installation: admission refuses an installation that another
//! inbox has added, in a stored update, in the same request, or in a request
//! that commits first.

use super::*;
use support::locks::{publish_task, wait_for_lock};
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_cryptography::utils::generate_local_wallet;

/// The creation update of a new wallet inbox, in which the wallet adds
/// `installation`, and the inbox id.
async fn creation(installation: &XmtpInstallationCredential) -> (String, IdentityUpdate) {
    let wallet = generate_local_wallet();
    let inbox_id = wallet.get_inbox_id(0);
    let mut request = SignatureRequestBuilder::new(&inbox_id)
        .create_inbox(wallet.identifier(), 0)
        .add_association(member(installation), wallet.identifier().into())
        .build();
    add_wallet_signature(&mut request, &wallet).await;
    add_installation_key_signature(&mut request, installation).await;
    let update = request
        .build_identity_update()
        .expect("the request is signed")
        .into();
    (inbox_id, update)
}

fn member(installation: &XmtpInstallationCredential) -> MemberIdentifier {
    MemberIdentifier::installation(installation.public_bytes().to_vec())
}

/// Every recorded installation and the inbox that added it, as hex.
async fn owners(server: &TestServer) -> support::TestResult<Vec<(String, String)>> {
    let rows: Vec<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT installation_key, inbox_id FROM installation_association
         ORDER BY installation_key",
    )
    .fetch_all(&server.backend.store.primary)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(key, inbox)| (hex::encode(key), hex::encode(inbox)))
        .collect())
}

fn owner(installation: &XmtpInstallationCredential, inbox_id: &str) -> (String, String) {
    (
        hex::encode(installation.public_bytes()),
        inbox_id.to_owned(),
    )
}

/// A second inbox that adds an installation another inbox added is refused
/// at its own index, whether it adds the installation when it is created or
/// later, behind a valid update at a lower index, and nothing is stored.
// verifies: API-305
#[xmtp_common::test(unwrap_try = true)]
async fn installation_added_by_another_inbox_is_refused() {
    let server = TestServer::new(|_| {}).await?;
    let shared = XmtpInstallationCredential::new();
    let (first, created) = creation(&shared).await;
    server.publish(vec![identity_envelope(created)]).await?;
    // An installation cannot add another, so the second inbox's wallet does.
    let wallet = generate_local_wallet();
    let second = wallet.get_inbox_id(0);
    let mut created = SignatureRequestBuilder::new(&second)
        .create_inbox(wallet.identifier(), 0)
        .build();
    add_wallet_signature(&mut created, &wallet).await;
    server
        .publish(vec![identity_envelope(
            created.build_identity_update()?.into(),
        )])
        .await?;
    let mut added = SignatureRequestBuilder::new(&second)
        .add_association(member(&shared), wallet.identifier().into())
        .build();
    add_wallet_signature(&mut added, &wallet).await;
    add_installation_key_signature(&mut added, &shared).await;
    let added: IdentityUpdate = added.build_identity_update()?.into();
    let bystander = identity_history_with_passkey().await;
    let before = identity_rows(&server).await?;
    let recorded = owners(&server).await?;

    let (_, third) = creation(&shared).await;
    for (case, update) in [("creation", third), ("addition", added)] {
        assert_eq!(
            refused(&server, vec![bystander.history[0].clone(), update]).await?,
            (Some(1), Reason::InstallationTaken),
            "{case}"
        );
        assert_eq!(identity_rows(&server).await?, before, "{case}");
        assert_eq!(owners(&server).await?, recorded, "{case}");
    }
    assert!(recorded.contains(&owner(&shared, &first)));
    server.stop().await?;
}

/// Two inboxes that add one installation in one request: the lower index
/// passes the check, the later index is refused, and nothing is stored.
// verifies: API-305
#[xmtp_common::test(unwrap_try = true)]
async fn same_request_additions_of_one_installation_fail_at_the_later_index() {
    let server = TestServer::new(|_| {}).await?;
    let shared = XmtpInstallationCredential::new();
    let (_, first) = creation(&shared).await;
    let (_, second) = creation(&shared).await;
    let before = identity_rows(&server).await?;

    assert_eq!(
        refused(&server, vec![first, second]).await?,
        (Some(1), Reason::InstallationTaken)
    );
    assert_eq!(identity_rows(&server).await?, before);
    assert_eq!(owners(&server).await?, []);
    server.stop().await?;
}

/// Two requests that add one installation to two inboxes at once: the second
/// validates before the first commits, then waits for the identity lock. The
/// check under the lock sees the first addition, so one is stored and the
/// other is refused, not failed as an internal error.
// verifies: API-305
#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_additions_of_one_installation_store_one() {
    let server = TestServer::new(|_| {}).await?;
    let shared = XmtpInstallationCredential::new();
    let (first, winning) = creation(&shared).await;
    let (_, losing) = creation(&shared).await;
    let pool = &server.backend.store.primary;
    let mut blocker = pool.begin().await?;
    sqlx::query!("LOCK TABLE envelopes IN SHARE MODE")
        .execute(&mut *blocker)
        .await?;

    let winner = publish_task(&server, identity_envelope(winning));
    wait_for_lock(pool, "relation").await?;
    let loser = publish_task(&server, identity_envelope(losing));
    wait_for_lock(pool, "advisory").await?;
    blocker.commit().await?;

    winner.await??;
    let error = loser.await?.expect_err("the second addition is refused");
    assert_eq!(refusal(&error)?, (Some(0), Reason::InstallationTaken));
    assert_eq!(owners(&server).await?, [owner(&shared, &first)]);
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes")
        .fetch_one(pool)
        .await?;
    assert_eq!(stored, 1);
    server.stop().await?;
}

/// A revocation does not free an installation: another inbox that adds it
/// afterwards is still refused, and the inbox that added it may add it again.
// verifies: API-305
#[xmtp_common::test(unwrap_try = true)]
async fn revoked_installation_stays_with_the_inbox_that_added_it() {
    let server = TestServer::new(|_| {}).await?;
    let wallet = generate_local_wallet();
    let first = wallet.get_inbox_id(0);
    let shared = XmtpInstallationCredential::new();
    let mut created = SignatureRequestBuilder::new(&first)
        .create_inbox(wallet.identifier(), 0)
        .add_association(member(&shared), wallet.identifier().into())
        .build();
    add_wallet_signature(&mut created, &wallet).await;
    add_installation_key_signature(&mut created, &shared).await;
    server
        .publish(vec![identity_envelope(
            created.build_identity_update()?.into(),
        )])
        .await?;
    let mut revoked = SignatureRequestBuilder::new(&first)
        .revoke_association(wallet.identifier().into(), member(&shared))
        .build();
    add_wallet_signature(&mut revoked, &wallet).await;
    server
        .publish(vec![identity_envelope(
            revoked.build_identity_update()?.into(),
        )])
        .await?;

    let (_, second) = creation(&shared).await;
    assert_eq!(
        refused(&server, vec![second]).await?,
        (Some(0), Reason::InstallationTaken)
    );

    let mut readded = SignatureRequestBuilder::new(&first)
        .add_association(member(&shared), wallet.identifier().into())
        .build();
    add_wallet_signature(&mut readded, &wallet).await;
    add_installation_key_signature(&mut readded, &shared).await;
    server
        .publish(vec![identity_envelope(
            readded.build_identity_update()?.into(),
        )])
        .await?;
    assert_eq!(owners(&server).await?, [owner(&shared, &first)]);
    server.stop().await?;
}

/// An update that adds an installation and revokes it again still records
/// the addition, so another inbox that adds the installation later is
/// refused. The active-member diff of that update holds no installation.
// verifies: API-305
#[xmtp_common::test(unwrap_try = true)]
async fn installation_added_and_revoked_in_one_update_stays_taken() {
    let server = TestServer::new(|_| {}).await?;
    let wallet = generate_local_wallet();
    let first = wallet.get_inbox_id(0);
    let shared = XmtpInstallationCredential::new();
    let mut created = SignatureRequestBuilder::new(&first)
        .create_inbox(wallet.identifier(), 0)
        .add_association(member(&shared), wallet.identifier().into())
        .revoke_association(wallet.identifier().into(), member(&shared))
        .build();
    add_wallet_signature(&mut created, &wallet).await;
    add_installation_key_signature(&mut created, &shared).await;
    server
        .publish(vec![identity_envelope(
            created.build_identity_update()?.into(),
        )])
        .await?;

    let (_, second) = creation(&shared).await;
    assert_eq!(
        refused(&server, vec![second]).await?,
        (Some(0), Reason::InstallationTaken)
    );
    assert_eq!(owners(&server).await?, [owner(&shared, &first)]);
    server.stop().await?;
}
