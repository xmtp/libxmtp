//! Post-quantum and legacy key packages across a DM welcome.

use crate::builder::ClientBuilder;
use crate::context::CoreContext;
use crate::identity_lifecycle::test_support::rotate_and_upload_key_package;
use crate::utils::FullXmtpClient;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::group::{ConversationType, GroupQueryArgs};
use xmtp_id::key_package::{VerifiedKeyPackageV2, WrapperAlgorithm};
use xmtp_mls_common::group::DMMetadataOptions;

async fn get_key_package_from_network(client: &FullXmtpClient) -> VerifiedKeyPackageV2 {
    let mut kp_mapping = client
        .get_key_packages_for_installation_ids(vec![client.installation_public_key().to_vec()])
        .await
        .unwrap();

    kp_mapping
        .remove(client.installation_public_key().as_slice())
        .unwrap()
        .unwrap()
}

async fn get_latest_welcome(client: &FullXmtpClient) -> xmtp_proto::types::WelcomeMessage {
    let welcomes = client
        .context
        .api()
        .query_welcome_messages(client.context.installation_id())
        .await
        .unwrap();

    welcomes[0].clone()
}

#[xmtp_common::test]
async fn post_quantum_interop() {
    for [amal_has_pq, bola_has_pq] in [[true, false], [false, true], [true, true], [false, false]] {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let amal_api = amal.context.api();
        let amal_mls = amal.context.mls_storage();

        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_api = bola.context.api();
        let bola_mls = bola.context.mls_storage();

        // Give amal a post quantum key package and bola a legacy key package
        rotate_and_upload_key_package(amal.identity(), amal_api, amal_mls, amal_has_pq)
            .await
            .unwrap();
        rotate_and_upload_key_package(bola.identity(), bola_api, bola_mls, bola_has_pq)
            .await
            .unwrap();

        // Create a DM from Amal -> Bola
        // This should use Bola's XWingMLKEM512 key package
        amal.find_or_create_dm(
            bola.inbox_id().to_string(),
            Some(DMMetadataOptions::default()),
        )
        .await
        .unwrap();

        // Sync both clients
        amal.sync_welcomes().await.unwrap();
        bola.sync_welcomes().await.unwrap();

        // Get the DMs from the clients
        let query_args = GroupQueryArgs {
            conversation_type: Some(ConversationType::Dm),
            ..GroupQueryArgs::default()
        };

        let amal_convos = amal.list_conversations(query_args.clone()).unwrap();
        let bola_convos = bola.list_conversations(query_args).unwrap();

        assert_eq!(amal_convos.len(), 1);
        assert_eq!(bola_convos.len(), 1);

        let amal_key_package = get_key_package_from_network(&amal).await;
        let bola_key_package = get_key_package_from_network(&bola).await;

        assert_eq!(
            amal_key_package.wrapper_encryption().unwrap().is_some(),
            amal_has_pq
        );
        assert_eq!(
            bola_key_package.wrapper_encryption().unwrap().is_some(),
            bola_has_pq
        );

        // Get the welcome messages from the network
        let bola_welcome = get_latest_welcome(&bola).await;

        // Make sure the wrapper algorithms were set correctly in the Welcome messages
        let pq_algorithm = WrapperAlgorithm::XWingMLKEM768Draft6;
        let traditional_algorithm = WrapperAlgorithm::Curve25519;
        let bola_wrapper = bola_welcome.as_v1().unwrap().wrapper_algorithm;
        if bola_has_pq {
            assert_eq!(bola_wrapper, pq_algorithm.into());
        } else {
            assert_eq!(bola_wrapper, traditional_algorithm.into());
        }
    }
}
