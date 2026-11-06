//! The leaves of `groups/tests/test_metadata_permissions.rs` that need the client.

use crate::groups::PreconfiguredPolicies;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;
use xmtp_db::group::GroupQueryArgs;
use xmtp_mls_group_state::group::GroupMetadataOptions;
use xmtp_mls_group_state::group_mutable_metadata::{MessageDisappearingSettings, MetadataField};

#[xmtp_common::test]
async fn test_group_options() {
    let expected_group_message_disappearing_settings = MessageDisappearingSettings::new(100, 200);

    tester!(amal);

    let amal_group = amal
        .create_group(
            None,
            Some(GroupMetadataOptions {
                name: Some("Group Name".to_string()),
                image_url_square: Some("url".to_string()),
                description: Some("group description".to_string()),
                message_disappearing_settings: Some(expected_group_message_disappearing_settings),
                app_data: None,
            }),
        )
        .unwrap();

    let binding = amal_group.mutable_metadata().expect("msg");
    let amal_group_name: &String = binding
        .attributes()
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    let amal_group_image_url: &String = binding
        .attributes()
        .get(&MetadataField::GroupImageUrlSquare.to_string())
        .unwrap();
    let amal_group_description: &String = binding
        .attributes()
        .get(&MetadataField::Description.to_string())
        .unwrap();
    let amal_group_message_disappear_from_ns = binding
        .attributes()
        .get(&MetadataField::MessageDisappearFromNS.to_string())
        .unwrap();
    let amal_group_message_disappear_in_ns = binding
        .attributes()
        .get(&MetadataField::MessageDisappearInNS.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "Group Name");
    assert_eq!(amal_group_image_url, "url");
    assert_eq!(amal_group_description, "group description");
    assert_eq!(
        amal_group_message_disappear_from_ns.clone(),
        expected_group_message_disappearing_settings
            .from_ns
            .to_string()
    );
    assert_eq!(
        amal_group_message_disappear_in_ns.clone(),
        expected_group_message_disappearing_settings
            .in_ns
            .to_string()
    );
}

#[xmtp_common::test]
async fn test_group_mutable_data() {
    tester!(amal);
    tester!(bola);

    // Create a group and verify it has the default group name
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
    assert_eq!(
        group_mutable_metadata
            .attributes()
            .get(&MetadataField::MinimumSupportedProtocolVersion.to_string()),
        Some(&xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION.to_string())
    );
    // A new group stores no commit-log signer.
    assert_eq!(
        group_mutable_metadata
            .attributes()
            .get(&MetadataField::CommitLogSigner.to_string()),
        None
    );
    for field in [
        MetadataField::GroupName,
        MetadataField::Description,
        MetadataField::GroupImageUrlSquare,
        MetadataField::AppData,
    ] {
        assert_eq!(
            group_mutable_metadata
                .attributes()
                .get(&field.to_string())
                .map(String::as_str),
            Some(""),
            "an unset bounded-string field reads as an empty string"
        );
    }

    // Add bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
    bola.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let group_mutable_metadata = bola_group.mutable_metadata().unwrap();
    for field in [
        MetadataField::GroupName,
        MetadataField::Description,
        MetadataField::GroupImageUrlSquare,
        MetadataField::AppData,
    ] {
        assert_eq!(
            group_mutable_metadata
                .attributes()
                .get(&field.to_string())
                .map(String::as_str),
            Some(""),
            "an unset bounded-string field reads as an empty string"
        );
    }

    // Update group name
    amal_group
        .update_group_name("New Group Name 1".to_string())
        .await
        .unwrap();

    amal_group
        .send_message("hello".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Verify amal group sees update
    amal_group.sync().await.unwrap();
    let binding = amal_group.mutable_metadata().expect("msg");
    let amal_group_name: &String = binding
        .attributes()
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "New Group Name 1");

    // Verify bola group sees update
    bola_group.sync().await.unwrap();
    let binding = bola_group.mutable_metadata().expect("msg");
    let bola_group_name: &String = binding
        .attributes()
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(bola_group_name, "New Group Name 1");

    // Verify that bola can not update the group name since they are not the creator
    bola_group
        .update_group_name("New Group Name 2".to_string())
        .await
        .expect_err("expected err");

    // Verify bola group does not see an update
    bola_group.sync().await.unwrap();
    let binding = bola_group.mutable_metadata().expect("msg");
    let bola_group_name: &String = binding
        .attributes()
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(bola_group_name, "New Group Name 1");
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_group_mutable_data_group_permissions() {
    tester!(amal);
    tester!(bola);

    // Create a group and verify it has the default group name
    let policy_set = Some(PreconfiguredPolicies::Default.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
    assert_eq!(
        group_mutable_metadata
            .attributes()
            .get(&MetadataField::GroupName.to_string())
            .map(String::as_str),
        Some(""),
        "an unset bounded-string field reads as an empty string"
    );

    // Add bola to the group
    amal_group
        .add_members_by_identity(&[bola.identifier()])
        .await
        .unwrap();
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let group_mutable_metadata = bola_group.mutable_metadata().unwrap();
    assert_eq!(
        group_mutable_metadata
            .attributes()
            .get(&MetadataField::GroupName.to_string())
            .map(String::as_str),
        Some(""),
        "an unset bounded-string field reads as an empty string"
    );

    // Update group name
    amal_group
        .update_group_name("New Group Name 1".to_string())
        .await
        .unwrap();

    // Verify amal group sees update
    amal_group.sync().await.unwrap();
    let binding = amal_group.mutable_metadata().unwrap();
    let amal_group_name: &String = binding
        .attributes()
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "New Group Name 1");

    // Verify bola group sees update
    bola_group.sync().await.unwrap();
    let binding = bola_group.mutable_metadata().expect("msg");
    let bola_group_name: &String = binding
        .attributes()
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(bola_group_name, "New Group Name 1");

    // Verify that bola CAN update the group name since everyone is admin for this group
    bola_group
        .update_group_name("New Group Name 2".to_string())
        .await
        .expect("non creator failed to update group name");

    // Verify amal group sees an update
    amal_group.sync().await.unwrap();
    let binding = amal_group.mutable_metadata().expect("msg");
    let amal_group_name: &String = binding
        .attributes()
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "New Group Name 2");
}
