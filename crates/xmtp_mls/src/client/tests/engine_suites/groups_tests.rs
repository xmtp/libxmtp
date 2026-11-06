//! The leaves of `groups/tests/mod.rs` that need the client.

use crate::context::CoreContext;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;
use crate::utils::fixtures::{alix, bola, caro};
use crate::utils::{ClientTester, TestMlsGroup};
use rstest::*;
use xmtp_content_types::ContentCodec;
use xmtp_db::consent_record::ConsentState;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_mls_group_state::group::GroupMetadataOptions;

#[xmtp_common::test]
async fn test_create_group_with_app_data() {
    tester!(amal);

    let initial_app_data = "Initial app data from options".to_string();

    // Create a group with app_data set through GroupMetadataOptions
    let group = amal
        .create_group(
            None,
            Some(GroupMetadataOptions {
                name: Some("Test Group".to_string()),
                description: Some("Test Description".to_string()),
                image_url_square: None,
                message_disappearing_settings: None,
                app_data: Some(initial_app_data.clone()),
            }),
        )
        .unwrap();

    group.sync().await.unwrap();

    // Verify the app_data was set correctly
    let retrieved_app_data = group.app_data().unwrap();
    assert_eq!(retrieved_app_data, initial_app_data);

    // Verify we can also update it
    let updated_app_data = "Updated app data".to_string();
    group
        .update_app_data(updated_app_data.clone(), None)
        .await
        .unwrap();
    group.sync().await.unwrap();

    let final_app_data = group.app_data().unwrap();
    assert_eq!(final_app_data, updated_app_data);
}

#[xmtp_common::test]
async fn test_create_group_with_default_app_data() {
    tester!(amal);

    // Create a group without specifying app_data (should default to empty string)
    let group = amal
        .create_group(None, Some(GroupMetadataOptions::default()))
        .unwrap();

    group.sync().await.unwrap();

    // Verify the app_data defaults to empty string
    let retrieved_app_data = group.app_data().unwrap();
    assert_eq!(retrieved_app_data, "");
}

#[xmtp_common::test]
async fn test_dm_creation() {
    tester!(amal);
    tester!(bola);
    tester!(caro);

    // Amal creates a dm group targeting bola
    let amal_dm = amal
        .find_or_create_dm(bola.inbox_id().to_string(), None)
        .await
        .unwrap();

    // Amal can not add caro to the dm group
    let result = amal_dm.add_members(&[caro.inbox_id()]).await;
    assert!(result.is_err());

    // Bola is already a member
    let result = amal_dm
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
        .await;
    assert!(result.is_err());
    amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(members.len(), 2);

    // Bola can message amal
    let _ = bola.sync_welcomes().await;
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

    let bola_dm: &TestMlsGroup = bola_groups.first().unwrap();
    bola_dm
        .send_message(b"test one", SendMessageOpts::default())
        .await
        .unwrap();

    // Amal sync and reads message
    amal_dm.sync().await.unwrap();
    let messages = amal_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);
    let message = messages.last().unwrap();
    assert_eq!(message.decrypted_message_bytes, b"test one");

    // Amal can not remove bola
    let result = amal_dm.remove_members(&[bola.inbox_id()]).await;
    assert!(result.is_err());
    amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(members.len(), 2);

    // Neither Amal nor Bola is an admin or super admin
    amal_dm.sync().await.unwrap();
    bola_dm.sync().await.unwrap();
    let is_amal_admin = amal_dm.is_admin(amal.inbox_id().to_string()).unwrap();
    let is_bola_admin = amal_dm.is_admin(bola.inbox_id().to_string()).unwrap();
    let is_amal_super_admin = amal_dm.is_super_admin(amal.inbox_id().to_string()).unwrap();
    let is_bola_super_admin = amal_dm.is_super_admin(bola.inbox_id().to_string()).unwrap();
    assert!(!is_amal_admin);
    assert!(!is_bola_admin);
    assert!(!is_amal_super_admin);
    assert!(!is_bola_super_admin);
}

// verifies: CONS-057
#[cfg_attr(target_arch = "wasm32", ignore)]
#[rstest]
#[xmtp_common::test]
#[awt]
async fn test_get_and_set_consent(
    #[future] alix: ClientTester,
    #[future] bola: ClientTester,
    #[future] caro: ClientTester,
) {
    let alix_group = alix.create_group(None, None).unwrap();

    // group consent state should be allowed if user created it
    assert_eq!(alix_group.consent_state().unwrap(), ConsentState::Allowed);

    alix_group
        .update_consent_state(ConsentState::Denied)
        .unwrap();
    assert_eq!(alix_group.consent_state().unwrap(), ConsentState::Denied);

    alix_group.add_members(&[bola.inbox_id()]).await.unwrap();

    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    // group consent state should default to unknown for users who did not create the group
    assert_eq!(bola_group.consent_state().unwrap(), ConsentState::Unknown);

    bola_group
        .send_message("hi from bola".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // group consent state should be allowed if user sends a message to the group
    assert_eq!(bola_group.consent_state().unwrap(), ConsentState::Allowed);

    // A send in an allowed conversation stores no record.
    use crate::subscriptions::internal::InternalEvent::PreferencesChanged;
    let preferences = bola.context.events().subscribe(
        xmtp_events::EventFilter::default()
            .with_internal(|e| matches!(e, PreferencesChanged { .. })),
        Some(16),
    );
    bola_group
        .send_message("again from bola".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    assert!(preferences.drain().is_empty());

    alix_group.add_members(&[caro.inbox_id()]).await.unwrap();

    caro.sync_welcomes().await.unwrap();
    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();

    let text = xmtp_content_types::text::TextCodec::encode("hi from caro".into()).unwrap();
    let message_id = caro_group
        .send_message_optimistic(
            &xmtp_content_types::encoded_content_to_bytes(text),
            SendMessageOpts::default(),
        )
        .unwrap();

    // Queueing the message allows, before it is published.
    assert_eq!(caro_group.consent_state().unwrap(), ConsentState::Allowed);

    // A deny made before the publish completes stands, and a retry that
    // reuses the queued intent does not replace it.
    caro_group
        .update_consent_state(ConsentState::Denied)
        .unwrap();
    caro_group
        .publish_stored_message(&message_id)
        .await
        .unwrap();
    assert_eq!(caro_group.consent_state().unwrap(), ConsentState::Denied);
    caro_group.publish_messages().await.unwrap();
    assert_eq!(caro_group.consent_state().unwrap(), ConsentState::Denied);

    // Deleting a message does not allow.
    caro_group.delete_message(message_id).unwrap();
    assert_eq!(caro_group.consent_state().unwrap(), ConsentState::Denied);
}
