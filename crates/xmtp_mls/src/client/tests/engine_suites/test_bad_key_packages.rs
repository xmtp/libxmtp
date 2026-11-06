//! The leaves of `groups/tests/test_bad_key_packages.rs` that need the client.

use crate::context::CoreContext;
use crate::groups::GroupError;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;
use crate::utils::TestMlsGroup;
use prost::Message;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group_message::MsgQueryArgs;

#[cfg(not(target_arch = "wasm32"))]
// verifies: GMOD-056
#[xmtp_common::test(unwrap_try = true)]
async fn test_create_group_with_member_all_malformed_installations() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
    tester!(alix);

    // bola has two installations
    tester!(bola_1);
    tester!(bola_2, from: bola_1);

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![
            bola_1.context.installation_id().to_vec(),
            bola_2.context.installation_id().to_vec(),
        ]),
    );

    let result = alix
        .create_group_with_identifiers(&[bola_1.identifier()], None, None)
        .await;
    assert!(
        result.is_err(),
        "Group creation should fail when all installations have bad key packages"
    );

    assert!(alix.find_groups(GroupQueryArgs::default())?.is_empty());
    bola_1.sync_welcomes().await?;
    bola_2.sync_welcomes().await?;

    let bola_1_groups = bola_1.find_groups(GroupQueryArgs::default())?;
    let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default())?;

    assert_eq!(
        bola_1_groups.len(),
        0,
        "Bola_1 should have no groups after failed creation"
    );
    assert_eq!(
        bola_2_groups.len(),
        0,
        "Bola_2 should have no groups after failed creation"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_dm_creation_with_user_all_malformed_installations() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
    // 1) Prepare clients
    tester!(amal);
    tester!(bola_1);
    tester!(bola_2, from: bola_1);

    // 2) Mark all of Bola's installations as malformed
    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![
            bola_1.context.installation_id().to_vec(),
            bola_2.context.installation_id().to_vec(),
        ]),
    );

    // 3) Attempt to create the DM group, which should fail

    let result = amal
        .find_or_create_dm_by_identity(bola_1.identifier(), None)
        .await;

    // 4) Ensure DM creation fails with the correct error
    assert!(result.is_err());

    // 5) Ensure Bola_1 does not have any groups
    bola_1.sync_welcomes().await.unwrap();
    let bola_1_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(
        bola_1_groups.len(),
        0,
        "Bola_1 should have no DM group due to malformed key package"
    );

    // 6) Ensure Bola_2 does not have any groups
    bola_2.sync_welcomes().await.unwrap();
    let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(
        bola_2_groups.len(),
        0,
        "Bola_2 should have no DM group due to malformed key package"
    );
}

/// A key package that advertises only a pointee AEAD value this client does
/// not define is usable, because the value counts as absent. The backend
/// admits it, and an add and a refresh both add the installation and send it
/// a Welcome.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
// verifies: JOIN-090, JOIN-093
async fn unknown_pointer_aead_value_is_ignored() {
    use crate::groups::test_support::key_package_with_extensions;
    use openmls::prelude::{Extension, UnknownExtension};
    use openmls_traits::OpenMlsProvider;
    use xmtp_configuration::WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID;
    use xmtp_id::key_package::VerifiedKeyPackageV2;
    use xmtp_mls_validation::commit::extract_group_membership;
    use xmtp_proto::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadTypesExtension;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    // Bo adds a second installation, whose package advertises only an AEAD
    // value this client does not define.
    tester!(bo2, from: bo, disable_workers);
    let bo2_id = bo2.context.installation_id();
    let pointee_aead = WelcomePointeeEncryptionAeadTypesExtension {
        supported_aead_types: vec![i32::MAX],
    };
    let bytes = key_package_with_extensions(
        &bo2.context,
        xmtp_cryptography::configuration::CIPHERSUITE,
        vec![Extension::Unknown(
            WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID,
            UnknownExtension(pointee_aead.encode_to_vec()),
        )],
    );
    bo2.context.api().upload_key_package(bytes.clone()).await?;

    let fetched = alix.context.api().fetch_key_packages(&[bo2_id]).await?;
    let served = fetched.get(&bo2_id).unwrap().as_ref().unwrap();
    assert_eq!(served.key_package_tls_serialized, bytes);
    let verified = VerifiedKeyPackageV2::from_bytes(
        bo2.context.mls_provider().crypto(),
        &served.key_package_tls_serialized,
    )?;
    assert!(verified.pointee_aead.supported_aead_types.is_empty());

    let caro_group = caro
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let failed_installations = caro_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(
                extract_group_membership(mls_group.extensions())?.failed_installations,
            )
        })
        .await?;
    assert!(failed_installations.is_empty());
    let welcomes = caro.context.api().query_welcome_messages(bo2_id).await?;
    assert_eq!(
        welcomes.len(),
        1,
        "the add must send the installation a Welcome"
    );

    alix_group.update_installations().await?;
    let welcomes = alix.context.api().query_welcome_messages(bo2_id).await?;
    assert_eq!(
        welcomes.len(),
        2,
        "the refresh must add the installation too"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_dm_creation_with_user_two_installations_one_malformed() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
    // 1) Prepare clients
    tester!(amal);
    tester!(bola_1);
    tester!(bola_2, from: bola_1);

    // 2) Mark bola_2's installation as malformed
    assert_ne!(
        bola_1.context.installation_id(),
        bola_2.context.installation_id()
    );
    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bola_2.context.installation_id().to_vec()]),
    );

    // 3) Amal creates a DM group targeting Bola
    let amal_dm = amal
        .find_or_create_dm(bola_1.inbox_id().to_string(), None)
        .await
        .unwrap();

    // 4) Ensure the DM is created with only 2 members (Amal + one valid Bola installation)
    // amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(
        members.len(),
        2,
        "DM should contain only Amal and one valid Bola installation"
    );

    // 5) Bola_1 syncs and confirms it has the DM
    bola_1.sync_welcomes().await.unwrap();
    // xmtp_common::time::sleep(std::time::Duration::from_secs(4)).await;

    let bola_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();

    assert_eq!(bola_groups.len(), 1, "Bola_1 should see the DM group");

    let bola_1_dm: &TestMlsGroup = bola_groups.first().unwrap();
    bola_1_dm.sync().await.unwrap();

    // 6) Ensure Bola_2 does NOT have the group
    bola_2.sync_welcomes().await.unwrap();
    let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(
        bola_2_groups.len(),
        0,
        "Bola_2 should not have the DM group due to malformed key package"
    );

    // 7) Send a message from Amal to Bola_1
    let message_text = b"Hello from Amal";
    amal_dm
        .send_message(message_text, SendMessageOpts::default())
        .await
        .unwrap();

    // 8) Sync both sides and check message delivery
    amal_dm.sync().await.unwrap();
    bola_1_dm.sync().await.unwrap();

    // Verify Bola_1 received the message
    let messages_bola_1 = bola_1_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(
        messages_bola_1.len(),
        2,
        "Bola_1 should have received Amal's message"
    );

    let last_message = messages_bola_1.last().unwrap();
    assert_eq!(
        last_message.decrypted_message_bytes, message_text,
        "Bola_1 should receive the correct message"
    );

    // 9) Bola_1 replies, and Amal confirms receipt
    let reply_text = b"Hey Amal!";
    bola_1_dm
        .send_message(reply_text, SendMessageOpts::default())
        .await
        .unwrap();

    amal_dm.sync().await.unwrap();
    let messages_amal = amal_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages_amal.len(), 3, "Amal should receive Bola_1's reply");

    let last_message_amal = messages_amal.last().unwrap();
    assert_eq!(
        last_message_amal.decrypted_message_bytes, reply_text,
        "Amal should receive the correct reply from Bola_1"
    );

    // 10) Ensure only valid installations are considered for the DM
    assert_eq!(
        amal_dm.members().await.unwrap().len(),
        2,
        "Only Amal and Bola_1 should be in the DM"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_add_inbox_with_bad_installation_to_group() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

    tester!(alix);
    tester!(caro);
    tester!(bo_1);
    tester!(bo_2, from: bo_1);

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bo_1.context.installation_id().to_vec()]),
    );

    let group = alix
        .create_group_with_identifiers(&[caro.identifier()], None, None)
        .await
        .unwrap();

    let _ = group.add_members_by_identity(&[bo_1.identifier()]).await;

    bo_2.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    let bo_2_groups = bo_2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bo_2_groups.len(), 1);
    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(caro_groups.len(), 1);
    let alix_groups = alix.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(alix_groups.len(), 1);
    assert!(crate::utils::test_mocks_helpers::key_package_injected(
        &bo_1.context.installation_id().to_vec()
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_add_inbox_with_good_installation_to_group_with_bad_installation() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

    tester!(alix);
    tester!(bo_1);
    tester!(bo_2, from: bo_1);
    tester!(caro);

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bo_1.context.installation_id().to_vec()]),
    );

    let group = alix
        .create_group_with_identifiers(&[bo_1.identifier()], None, None)
        .await
        .unwrap();

    let _ = group.add_members_by_identity(&[caro.identifier()]).await;

    caro.sync_welcomes().await.unwrap();
    bo_2.sync_welcomes().await.unwrap();
    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(caro_groups.len(), 1);
    let bo_groups = bo_2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bo_groups.len(), 1);
    let alix_groups = alix.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(alix_groups.len(), 1);
    assert!(crate::utils::test_mocks_helpers::key_package_injected(
        &bo_1.context.installation_id().to_vec()
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_remove_inbox_with_good_installation_from_group_with_bad_installation() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

    tester!(alix_1);
    tester!(alix_2, from: alix_1);
    tester!(bo);
    tester!(caro);

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![alix_2.context.installation_id().to_vec()]),
    );

    let group = alix_1
        .create_group_with_identifiers(&[bo.identifier(), caro.identifier()], None, None)
        .await
        .unwrap();

    assert_eq!(group.members().await.unwrap().len(), 3);
    let _ = group.remove_members_by_identity(&[caro.identifier()]).await;

    caro.sync_welcomes().await.unwrap();
    bo.sync_welcomes().await.unwrap();
    group.sync().await.unwrap();

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();
    assert!(!caro_group.is_active().unwrap());
    let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();
    bo_group.sync().await.unwrap();
    assert_eq!(bo_group.members().await.unwrap().len(), 2);
    assert_eq!(group.members().await.unwrap().len(), 2);
    assert!(crate::utils::test_mocks_helpers::key_package_injected(
        &alix_2.context.installation_id().to_vec()
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_remove_inbox_with_bad_installation_from_group() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

    tester!(alix);
    tester!(bo_1);
    tester!(bo_2, from: bo_1);
    tester!(caro);

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bo_1.context.installation_id().to_vec()]),
    );

    let group = alix
        .create_group_with_identifiers(&[bo_1.identifier(), caro.identifier()], None, None)
        .await
        .unwrap();

    group.sync().await.unwrap();

    let message_from_alix = b"Hello from Alix";
    group
        .send_message(message_from_alix, SendMessageOpts::default())
        .await
        .unwrap();

    bo_2.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    group.sync().await.unwrap();

    let bo_groups = bo_2.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();
    bo_group.sync().await.unwrap();
    let bo_msgs = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_msgs.len(), 2);
    assert_eq!(bo_msgs[1].decrypted_message_bytes, message_from_alix);

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();
    let caro_msgs = caro_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(caro_msgs.len(), 2);
    assert_eq!(caro_msgs[1].decrypted_message_bytes, message_from_alix);

    // Bo replies before removal
    let bo_reply = b"Hey Alix!";
    bo_group
        .send_message(bo_reply, SendMessageOpts::default())
        .await
        .unwrap();

    group.sync().await.unwrap();
    let group_msgs = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(group_msgs.len(), 3);
    assert_eq!(group_msgs.last().unwrap().decrypted_message_bytes, bo_reply);

    // Remove Bo
    group
        .remove_members_by_identity(&[bo_2.identifier()])
        .await
        .unwrap();

    bo_2.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    group.sync().await.unwrap();

    // Bo should no longer be active
    bo_group.sync().await.unwrap();
    assert!(!bo_group.is_active().unwrap());

    let post_removal_msg = b"Caro, just us now!";
    group
        .send_message(post_removal_msg, SendMessageOpts::default())
        .await
        .unwrap();
    let caro_post_removal_msg = b"Nice!";
    caro_group
        .send_message(caro_post_removal_msg, SendMessageOpts::default())
        .await
        .unwrap();

    caro_group.sync().await.unwrap();
    let caro_msgs = caro_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(caro_msgs.len(), 6);
    assert_eq!(
        caro_msgs.last().unwrap().decrypted_message_bytes,
        caro_post_removal_msg
    );
    group.sync().await.unwrap();
    let alix_msgs = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(alix_msgs.len(), 6);
    assert_eq!(
        alix_msgs.last().unwrap().decrypted_message_bytes,
        caro_post_removal_msg
    );

    let bo_msgs = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(
        bo_msgs.len(),
        4,
        "Bo should not receive messages after being removed"
    );

    assert_eq!(caro_group.members().await.unwrap().len(), 2);
    assert_eq!(group.members().await.unwrap().len(), 2);
    assert!(crate::utils::test_mocks_helpers::key_package_injected(
        &bo_1.context.installation_id().to_vec()
    ));
}
