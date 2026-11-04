//! A group with a malformed key package left out, then a revocation of an
//! installation of the same inbox.

#[cfg(not(target_arch = "wasm32"))]
use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestPeer};
#[cfg(not(target_arch = "wasm32"))]
use {
    crate::groups::test_support::get_association_state,
    xmtp_id::associations::test_utils::add_wallet_signature,
};

/// Creates a group of `client`'s inbox alone, so it adds the inbox's other
/// installations, and returns its failed installations and leaf keys.
#[cfg(not(target_arch = "wasm32"))]
async fn own_installation_group(client: &EngineTestPeer) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let group = client.create_group(None, None).unwrap();
    group.add_missing_installations().await.unwrap();
    group
        .load_mls_group_with_lock_async(async |mls| {
            Ok::<_, crate::groups::GroupError>((
                xmtp_mls_validation::commit::extract_group_membership(mls.extensions())?
                    .failed_installations,
                mls.members().map(|member| member.signature_key).collect(),
            ))
        })
        .await
        .unwrap()
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "multi_thread")]
pub async fn revoke_installation_with_malformed_keypackage() {
    use xmtp_mls_identity::identity_lifecycle::test_mocks::set_test_mode_upload_malformed_keypackage;

    let client1 = EngineTestPeer::new_with(|o| o.triggers = true).await;
    let wallet = client1.owner.clone();
    let client2 = client1.new_installation_with(|o| o.triggers = true).await;

    let association_state = get_association_state(&client1, client1.inbox_id()).await;
    // Ensure there are two installations on the inbox
    assert_eq!(association_state.installation_ids().len(), 2);

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![client2.installation_public_key().to_vec()]),
    );
    let (failed, leaves) = own_installation_group(&client1).await;
    assert!(failed.contains(&client2.installation_public_key().to_vec()));
    assert!(!leaves.contains(&client2.installation_public_key().to_vec()));

    // Now revoke the second client
    let mut revoke_installation_request = client1
        .identity_updates()
        .revoke_installations(vec![client2.installation_public_key().to_vec()])
        .await
        .unwrap();
    add_wallet_signature(&mut revoke_installation_request, &wallet).await;
    client1
        .identity_updates()
        .apply_signature_request(revoke_installation_request)
        .await
        .unwrap();

    // Make sure there is only one installation on the inbox
    let association_state = get_association_state(&client1, client1.inbox_id()).await;
    assert_eq!(association_state.installation_ids().len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "multi_thread")]
pub async fn revoke_good_installation_with_other_malformed_keypackage() {
    use xmtp_mls_identity::identity_lifecycle::test_mocks::set_test_mode_upload_malformed_keypackage;

    let client1 = EngineTestPeer::new_with(|o| o.triggers = true).await;
    let wallet = client1.owner.clone();
    let client2 = client1.new_installation_with(|o| o.triggers = true).await;
    let client3 = client1.new_installation_with(|o| o.triggers = true).await;

    let association_state = get_association_state(&client1, client1.inbox_id()).await;
    // Ensure there are two installations on the inbox
    assert_eq!(association_state.installation_ids().len(), 3);

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![client2.installation_public_key().to_vec()]),
    );
    let (failed, leaves) = own_installation_group(&client1).await;
    assert!(failed.contains(&client2.installation_public_key().to_vec()));
    assert!(!leaves.contains(&client2.installation_public_key().to_vec()));
    assert!(leaves.contains(&client3.installation_public_key().to_vec()));

    // Now revoke the second client
    let mut revoke_installation_request = client1
        .identity_updates()
        .revoke_installations(vec![client3.installation_public_key().to_vec()])
        .await
        .unwrap();
    add_wallet_signature(&mut revoke_installation_request, &wallet).await;
    client1
        .identity_updates()
        .apply_signature_request(revoke_installation_request)
        .await
        .unwrap();

    // Make sure there is only one installation on the inbox
    let association_state = get_association_state(&client1, client1.inbox_id()).await;
    assert_eq!(association_state.installation_ids().len(), 2);
}
