//! Own-inbox refreshes through `Client::inbox_state` and the installation events they emit.

use super::*;
use crate::utils::FullXmtpClient;
use xmtp_id::associations::test_utils::add_wallet_signature;

// verifies: EVENT-001
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
async fn direct_own_inbox_refresh_reports_each_installation_transition() {
    use xmtp_events::{ClientEvent, EventFilter, EventKind};
    let wallet = generate_local_wallet();
    let observer: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
    observer.inbox_state(false).await?;
    let events = observer.context.events().subscribe(
        EventFilter::new([
            EventKind::IdentityOwnInstallationAdded,
            EventKind::IdentityOwnInstallationRevoked,
        ]),
        Some(10),
    );

    let transient: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
    let actor: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
    let mut request = actor
        .identity_updates()
        .revoke_installations(vec![transient.installation_public_key().to_vec()])
        .await?;
    add_wallet_signature(&mut request, &wallet).await;
    actor
        .identity_updates()
        .apply_signature_request(request)
        .await?;

    observer.inbox_state(true).await?;
    let changes = events.drain();
    assert_eq!(changes.len(), 3);
    assert!(changes.iter().any(|event| matches!(
        &event.client,
        Some(ClientEvent::IdentityOwnInstallationAdded(added))
            if added.installation_key == transient.installation_public_key().to_vec()
    )));
    assert!(changes.iter().any(|event| matches!(
        &event.client,
        Some(ClientEvent::IdentityOwnInstallationRevoked(revoked))
            if revoked.installation_key == transient.installation_public_key().to_vec()
    )));
    assert!(changes.iter().any(|event| matches!(
        &event.client,
        Some(ClientEvent::IdentityOwnInstallationAdded(added))
            if added.installation_key == actor.installation_public_key().to_vec()
    )));
}

// verifies: EVENT-001
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_own_inbox_refresh_emits_one_addition() {
    use xmtp_events::{ClientEvent, EventFilter, EventKind};

    let wallet = generate_local_wallet();
    let observer: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
    observer.inbox_state(false).await?;
    let events = observer.context.events().subscribe(
        EventFilter::new([EventKind::IdentityOwnInstallationAdded]),
        Some(10),
    );
    let added: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;

    let (first, second) = tokio::join!(observer.inbox_state(true), observer.inbox_state(true));
    first?;
    second?;
    let additions = events.drain();
    assert!(matches!(additions.as_slice(), [event]
        if matches!(&event.client,
            Some(ClientEvent::IdentityOwnInstallationAdded(value))
                if value.installation_key == added.installation_public_key().to_vec()
        )
    ));
}
