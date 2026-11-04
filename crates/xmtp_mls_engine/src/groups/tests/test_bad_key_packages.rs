//! Groups containing malformed key packages.

#[cfg(not(target_arch = "wasm32"))]
use super::*;
#[cfg(not(target_arch = "wasm32"))]
use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestPeer, EngineTestPeerOps};

/// A member applies a commit that adds a key package which fails the
/// addressing checks. Only senders run those checks: members that disagreed
/// about a key package in a commit would fork the group. Alix skips them
/// here, as a client from before them would.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
// verifies: JOIN-090
async fn commit_adding_unusable_package_is_applied_by_members() {
    use crate::groups::mls_ext::WelcomePointersExtension;
    use crate::groups::test_support::key_package_with_extensions;
    use openmls::prelude::{Extension, KeyPackageIn, UnknownExtension, tls_codec::Deserialize};
    use openmls_traits::OpenMlsProvider;
    use xmtp_configuration::WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID;
    use xmtp_cryptography::configuration::MLS_PROTOCOL_VERSION;
    use xmtp_id::key_package::{VerifiedKeyPackageV2, WrapperEncryptionExtension};
    use xmtp_mls_identity::identity_lifecycle::test_mocks::set_test_mode_unchecked_key_package;
    use xmtp_proto::xmtp::identity::MlsCredential;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_group = alix
        .create_group_with_members(&[caro.inbox_id()], None, None)
        .await?;
    caro.sync_welcomes().await?;
    let caro_group = caro.group(&alix_group.group_id)?;

    // Bo's package carries a pointee AEAD extension that does not decode.
    let bytes = key_package_with_extensions(
        &bo.context,
        xmtp_cryptography::configuration::CIPHERSUITE,
        vec![Extension::Unknown(
            WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID,
            UnknownExtension(vec![0xff]),
        )],
    );
    let inner = KeyPackageIn::tls_deserialize_exact(&bytes)?
        .validate(bo.context.mls_provider().crypto(), MLS_PROTOCOL_VERSION)?;
    assert!(VerifiedKeyPackageV2::try_from(inner.clone()).is_err());
    set_test_mode_unchecked_key_package(VerifiedKeyPackageV2 {
        credential: MlsCredential {
            inbox_id: bo.inbox_id().to_string(),
        },
        installation_public_key: bo.installation_public_key().to_vec(),
        wrapper: WrapperEncryptionExtension::advertised_by(&inner)?,
        pointee_aead: WelcomePointersExtension::empty(),
        inner,
    });

    alix_group.add_members(&[bo.inbox_id()]).await?;
    assert!(
        xmtp_mls_identity::identity_lifecycle::test_mocks::key_package_injected(
            &bo.context.installation_id().to_vec()
        )
    );
    caro_group.sync().await?;
    let caro_members: Vec<String> = caro_group
        .members()
        .await?
        .into_iter()
        .map(|member| member.inbox_id)
        .collect();
    assert!(
        caro_members.contains(&bo.inbox_id().to_string()),
        "Caro must apply the commit that adds Bo"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn test_create_from_welcome_validation() {
    use crate::groups::app_data::stage_app_data_propose_and_commit;
    use prost::Message as _;
    use tls_codec::VLBytes;
    use xmtp_mls_group_state::{
        app_data::{
            component_id::ComponentId, components::tls_map_components::GroupMembershipComponent,
            typed::Component,
        },
        inbox_id::InboxId,
    };
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1, Version},
    };
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    let alix_group = alix.create_group(None, None).unwrap();
    let provider = alix.context.mls_provider();
    // Add a dictionary membership entry for an inbox that has no MLS leaf.
    // The welcome receiver must still reject this malformed membership view.
    let mut mls_group = alix_group
        .load_mls_group_with_lock(alix.context.mls_storage(), |mut mls_group| {
            let phantom = InboxId::from_hex(&"ff".repeat(32)).unwrap();
            let entry = GroupMembershipEntry {
                version: Some(Version::V1(V1 {
                    sequence_id: 1,
                    failed_installations: vec![],
                })),
            };
            let delta = TlsMapDelta::new().insert(phantom, VLBytes::new(entry.encode_to_vec()));
            let payload = <GroupMembershipComponent as Component>::encode_mutation(&delta).unwrap();
            stage_app_data_propose_and_commit(
                &mut mls_group,
                &provider,
                &alix.identity().installation_keys,
                &[],
                ComponentId::GROUP_MEMBERSHIP,
                payload,
            )
            .unwrap();
            mls_group.merge_pending_commit(&provider).unwrap();

            Ok(mls_group)
        })
        .unwrap();

    // Now add bo to the group
    force_add_member(&alix, &bo, &alix_group, &mut mls_group, &provider).await;

    // Bo should not be able to actually read this group
    bo.sync_welcomes().await.unwrap();
    let groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(groups.len(), 0);
    let topic = xmtp_db::incoming_envelope::StreamTopic {
        entity_id: bo.context.installation_id().to_vec(),
        kind: xmtp_db::incoming_envelope::NetworkEntityKind::Welcome,
    };
    let db = bo.context.db();
    let rejected = db.read_last_rejection(&topic)??;
    assert_eq!(rejected.code, "invalid_welcome");
    assert!(rejected.sequence_id > Cursor(0));
    assert!(db.pending_envelope(&topic, rejected.sequence_id)?.is_none());
    assert!(db.find_group(&alix_group.group_id)?.is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "multi_thread")]
async fn test_can_make_inbox_with_a_bad_key_package_an_admin() {
    use xmtp_mls_identity::identity_lifecycle::test_mocks::set_test_mode_upload_malformed_keypackage;

    // 1) Prepare clients
    let amal = EngineTestPeer::new().await;
    let charlie = EngineTestPeer::new().await;

    // Create a wallet for the user with a bad key package
    let bola = EngineTestPeer::new().await;
    // Mark bola's installation as having a malformed key package
    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bola.context.installation_id().to_vec()]),
    );

    // 2) Create a group with amal as the only member
    let amal_group = amal
        .create_group(
            Some(PreconfiguredPolicies::AdminsOnly.to_policy_set()),
            None,
        )
        .unwrap();
    amal_group.sync().await.unwrap();

    // 3) Add charlie to the group (normal member)
    let result = amal_group.add_members(&[charlie.inbox_id()]).await;
    assert!(result.is_ok());

    // 4) Initially fail to add bola since they only have one bad key package
    let result = amal_group.add_members(&[bola.inbox_id()]).await;
    assert!(result.is_err());

    // 5) Add a second installation for bola and try and re-add them
    let bola_2 = bola.new_installation().await;
    let result = amal_group.add_members(&[bola.inbox_id()]).await;
    assert!(result.is_ok());

    // 6) Test that bola can not perform an admin only action
    bola_2.sync_welcomes().await.unwrap();
    let binding = bola_2.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = binding.first().unwrap();
    bola_group.sync().await.unwrap();
    let result = bola_group
        .update_group_name("Bola's Group".to_string())
        .await;
    assert!(result.is_err());

    // 7) Test adding bola as an admin
    let result = amal_group
        .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
        .await;
    assert!(result.is_ok());

    // 8) Verify bola can perform an admin only action
    bola_group.sync().await.unwrap();
    let result = bola_group
        .update_group_name("Bola's Group".to_string())
        .await;
    assert!(result.is_ok());

    // 9) Verify we can remove bola as an admin
    let result = amal_group
        .update_admin_list(UpdateAdminListType::Remove, bola.inbox_id().to_string())
        .await;
    assert!(result.is_ok());

    // 10) Verify bola is not an admin
    let admins = amal_group.admin_list().unwrap();
    assert!(!admins.contains(&bola.inbox_id().to_string()));

    // 11) verify bola can't perform an admin only action
    bola_group.sync().await.unwrap();
    let result = bola_group
        .update_group_name("Bola's Group Forever".to_string())
        .await;
    assert!(result.is_err());
}
