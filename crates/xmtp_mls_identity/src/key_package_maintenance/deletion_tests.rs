use super::*;
use crate::identity_lifecycle::test_support::IdentityTestPeer;
use openmls::prelude::{KeyPackageBundle, KeyPackageRef};
use openmls_traits::{OpenMlsProvider, storage::StorageProvider};
use xmtp_db::xmtp_openmls_provider::XmtpOpenMlsProvider;
use xmtp_db::{MlsProviderExt, XmtpOpenMlsProviderRef};
use xmtp_id::key_package::VerifiedKeyPackageV2;
use xmtp_mls_core::context::CoreContext;

async fn get_key_package_from_network<C: CoreContext>(context: &C) -> VerifiedKeyPackageV2 {
    let installation_id = context.installation_id();
    let mut served = context
        .api()
        .fetch_key_packages(&[installation_id])
        .await
        .expect("a key package fetch");
    let package = served
        .remove(&installation_id)
        .expect("an entry for the installation")
        .expect("a served key package");
    let verified = VerifiedKeyPackageV2::from_bytes(
        &XmtpOpenMlsProvider::<()>::new_crypto(),
        &package.key_package_tls_serialized,
    )
    .expect("a valid key package");
    assert_eq!(installation_id, verified.installation_public_key);
    verified
}

/// Look up the key package hash ref by public init key
fn get_hash_ref(provider: &impl MlsProviderExt, pub_key: &[u8]) -> Option<KeyPackageRef> {
    provider
        .key_store()
        .read(KEY_PACKAGE_REFERENCES, pub_key)
        .expect("a key reference read")
}

fn get_pq_private_key(provider: &impl MlsProviderExt, hash_ref: &[u8]) -> Option<Vec<u8>> {
    provider
        .key_store()
        .wrapper_private_key(hash_ref)
        .expect("a wrapper private key read")
        .map(|key| key.to_vec())
}

#[xmtp_common::test]
async fn ensure_pq_keys_are_deleted() {
    let peer = IdentityTestPeer::new().await;
    let storage = peer.context.mls_storage();
    let provider = XmtpOpenMlsProviderRef::new(storage);

    // As long as we have `config::CREATE_PQ_KEY_PACKAGE_EXTENSION` set to false, we need to do this step to force a PQ key package to be created
    crate::identity_lifecycle::rotate_and_upload_key_package(
        peer.context.identity(),
        peer.context.api(),
        storage,
        true,
    )
    .await
    .unwrap();

    // Get the key package back from the network
    let starting_key_package = get_key_package_from_network(&peer.context).await;
    let starting_init_key = starting_key_package
        .inner
        .hpke_init_key()
        .tls_serialize_detached()
        .unwrap();

    // Make sure we can find the init key
    let init_key_hash_ref = get_hash_ref(&provider, &starting_init_key);
    assert!(init_key_hash_ref.is_some());

    // Make sure we can find the post quantum public key
    let pq_public_key = starting_key_package.wrapper_encryption().unwrap();
    assert!(pq_public_key.is_some());

    let pq_public_key_bytes = pq_public_key.unwrap().pub_key_bytes;
    let pq_hash_ref = get_hash_ref(
        &provider,
        &pq_public_key_bytes
            .as_slice()
            .tls_serialize_detached()
            .unwrap(),
    );
    assert!(pq_hash_ref.is_some());
    let pq_hash_ref_inner = pq_hash_ref.unwrap();

    // Make sure we can find the key package based on the post quantum public key
    let key_package_bundle: KeyPackageBundle = provider
        .storage()
        .key_package(&pq_hash_ref_inner)
        .unwrap()
        .unwrap();

    // Make sure we can find the private key based on the init key
    let serialized_hash_ref = xmtp_db::sql_key_store::codec::encode_key(
        KEY_PACKAGE_WRAPPER_PRIVATE_KEY,
        &init_key_hash_ref.unwrap(),
    )
    .unwrap();
    let pq_private_key = get_pq_private_key(&provider, &serialized_hash_ref);
    assert!(pq_private_key.is_some());

    // Now rotate the key package
    crate::identity_lifecycle::rotate_and_upload_key_package(
        peer.context.identity(),
        peer.context.api(),
        storage,
        CREATE_PQ_KEY_PACKAGE_EXTENSION,
    )
    .await
    .unwrap();
    nudge_deletion(&peer.context).unwrap();

    // Force deletion of the key package, even though it hasn't expired yet
    let serialized_key_package_hash_ref = xmtp_db::sql_key_store::codec::encode_key(
        KEY_PACKAGE_WRAPPER_PRIVATE_KEY,
        &key_package_bundle
            .key_package()
            .hash_ref(provider.crypto())
            .unwrap(),
    )
    .unwrap();
    delete_key_package(
        &peer.context,
        serialized_key_package_hash_ref,
        Some(pq_public_key_bytes.clone()),
    )
    .unwrap();

    // Now test to see if the private keys are deleted by doing the same steps as above
    let pq_hash_ref = get_hash_ref(
        &provider,
        &pq_public_key_bytes
            .as_slice()
            .tls_serialize_detached()
            .unwrap(),
    );
    assert!(pq_hash_ref.is_none());

    let pq_private_key = get_pq_private_key(&provider, &serialized_hash_ref);
    assert!(pq_private_key.is_none());

    let key_package_from_db: Option<KeyPackageBundle> =
        provider.storage().key_package(&pq_hash_ref_inner).unwrap();
    assert!(key_package_from_db.is_none());
}
