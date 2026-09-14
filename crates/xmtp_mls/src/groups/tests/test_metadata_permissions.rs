//! Mutable metadata and permission policy updates.

use super::*;

#[xmtp_common::test]
async fn test_group_permissions() {
    tester!(amal);
    tester!(bola);
    tester!(charlie);

    let amal_group = amal
        .create_group(
            Some(PreconfiguredPolicies::AdminsOnly.to_policy_set()),
            None,
        )
        .unwrap();
    // Add bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    let bola_group = receive_group_invite(&bola).await;
    bola_group.sync().await.unwrap();
    assert!(bola_group.add_members(&[charlie.inbox_id()]).await.is_err(),);
}

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
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    let amal_group_image_url: &String = binding
        .attributes
        .get(&MetadataField::GroupImageUrlSquare.to_string())
        .unwrap();
    let amal_group_description: &String = binding
        .attributes
        .get(&MetadataField::Description.to_string())
        .unwrap();
    let amal_group_message_disappear_from_ns = binding
        .attributes
        .get(&MetadataField::MessageDisappearFromNS.to_string())
        .unwrap();
    let amal_group_message_disappear_in_ns = binding
        .attributes
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
    assert!(group_mutable_metadata.attributes.len().eq(&5));
    assert!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty()
    );

    // Add bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
    bola.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let group_mutable_metadata = bola_group.mutable_metadata().unwrap();
    assert!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty()
    );

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
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "New Group Name 1");

    // Verify bola group sees update
    bola_group.sync().await.unwrap();
    let binding = bola_group.mutable_metadata().expect("msg");
    let bola_group_name: &String = binding
        .attributes
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
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(bola_group_name, "New Group Name 1");
}

#[xmtp_common::test]
async fn test_update_policies_empty_group() {
    tester!(amal);
    tester!(bola);

    // Create a group with amal and bola
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal
        .create_group_with_identifiers(
            &[bola.builder.owner.get_identifier().unwrap()],
            policy_set,
            None,
        )
        .await
        .unwrap();

    // Verify we can update the group name without syncing first
    amal_group
        .update_group_name("New Group Name 1".to_string())
        .await
        .unwrap();

    // Verify the name is updated
    amal_group.sync().await.unwrap();
    let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
    let group_name_1 = group_mutable_metadata
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(group_name_1, "New Group Name 1");

    // Create a group with just amal
    let policy_set_2 = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group_2 = amal.create_group(policy_set_2, None).unwrap();

    // A solo group still holds the `sequence_id: 0` placeholder. The
    // placeholder must not block a metadata update. This assertion expected
    // the failure before. See `test_starting_membership_sequence_id`.
    amal_group_2
        .update_group_name("New Group Name 2".to_string())
        .await
        .unwrap();

    // Verify the name is updated
    amal_group_2.sync().await.unwrap();
    let group_mutable_metadata = amal_group_2.mutable_metadata().unwrap();
    let group_name_2 = group_mutable_metadata
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(group_name_2, "New Group Name 2");
}

#[xmtp_common::test]
async fn test_update_group_image_url_square() {
    tester!(amal);

    // Create a group and verify it has the default group name
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
    assert!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupImageUrlSquare.to_string())
            .unwrap()
            .is_empty()
    );

    // Update group name
    amal_group
        .update_group_image_url_square("a url".to_string())
        .await
        .unwrap();

    // Verify amal group sees update
    amal_group.sync().await.unwrap();
    let binding = amal_group.mutable_metadata().expect("msg");
    let amal_group_image_url: &String = binding
        .attributes
        .get(&MetadataField::GroupImageUrlSquare.to_string())
        .unwrap();
    assert_eq!(amal_group_image_url, "a url");
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_update_group_message_expiration_settings() {
    tester!(amal);

    // Create a group and verify it has the default group name
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
    assert_eq!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::MessageDisappearInNS.to_string()),
        None
    );
    assert_eq!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::MessageDisappearFromNS.to_string()),
        None
    );

    // Update group name
    let expected_group_message_expiration_settings = MessageDisappearingSettings::new(100, 200);

    amal_group
        .update_conversation_message_disappearing_settings(
            expected_group_message_expiration_settings,
        )
        .await
        .unwrap();

    // Verify amal group sees update
    amal_group.sync().await.unwrap();
    let binding = amal_group.mutable_metadata().expect("msg");
    let amal_message_expiration_from_ms: &String = binding
        .attributes
        .get(&MetadataField::MessageDisappearFromNS.to_string())
        .unwrap();
    let amal_message_disappear_in_ns: &String = binding
        .attributes
        .get(&MetadataField::MessageDisappearInNS.to_string())
        .unwrap();
    assert_eq!(
        amal_message_expiration_from_ms.clone(),
        expected_group_message_expiration_settings
            .from_ns
            .to_string()
    );
    assert_eq!(
        amal_message_disappear_in_ns.clone(),
        expected_group_message_expiration_settings.in_ns.to_string()
    );
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
    assert!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty()
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
    assert!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty()
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
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "New Group Name 1");

    // Verify bola group sees update
    bola_group.sync().await.unwrap();
    let binding = bola_group.mutable_metadata().expect("msg");
    let bola_group_name: &String = binding
        .attributes
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
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "New Group Name 2");
}

#[xmtp_common::test]
async fn test_group_admin_list_update() {
    tester!(amal);
    tester!(bola);
    tester!(caro);
    tester!(charlie);

    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

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

    // Verify Amal is the only admin and super admin
    let admin_list = amal_group.admin_list().unwrap();
    let super_admin_list = amal_group.super_admin_list().unwrap();
    assert_eq!(admin_list.len(), 0);
    assert_eq!(super_admin_list.len(), 1);
    assert!(super_admin_list.contains(&amal.inbox_id().to_string()));

    // Verify that bola can not add caro because they are not an admin
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group: &TestMlsGroup = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    bola_group
        .add_members(&[caro.inbox_id()])
        .await
        .expect_err("expected err");

    // Add bola as an admin
    amal_group
        .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    assert_eq!(bola_group.admin_list().unwrap().len(), 1);
    assert!(
        bola_group
            .admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify that bola can now add caro because they are an admin
    bola_group.add_members(&[caro.inbox_id()]).await.unwrap();

    bola_group.sync().await.unwrap();

    // Verify that bola can not remove amal as a super admin, because
    // Remove admin is super admin only permissions
    bola_group
        .update_admin_list(
            UpdateAdminListType::RemoveSuper,
            amal.inbox_id().to_string(),
        )
        .await
        .expect_err("expected err");

    // Now amal removes bola as an admin
    amal_group
        .update_admin_list(UpdateAdminListType::Remove, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    assert_eq!(bola_group.admin_list().unwrap().len(), 0);
    assert!(
        !bola_group
            .admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify that bola can not add charlie because they are not an admin
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group: &TestMlsGroup = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    bola_group
        .add_members(&[charlie.inbox_id()])
        .await
        .expect_err("expected err");
}

#[xmtp_common::test]
async fn test_group_super_admin_list_update() {
    tester!(amal);
    tester!(bola);
    tester!(caro);

    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    // Add bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    // Verify Amal is the only super admin
    let admin_list = amal_group.admin_list().unwrap();
    let super_admin_list = amal_group.super_admin_list().unwrap();
    assert_eq!(admin_list.len(), 0);
    assert_eq!(super_admin_list.len(), 1);
    assert!(super_admin_list.contains(&amal.inbox_id().to_string()));

    // Verify that bola can not add caro as an admin because they are not a super admin
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

    assert_eq!(bola_groups.len(), 1);
    let bola_group: &TestMlsGroup = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    bola_group
        .update_admin_list(UpdateAdminListType::Add, caro.inbox_id().to_string())
        .await
        .expect_err("expected err");

    // Add bola as a super admin
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    assert_eq!(bola_group.super_admin_list().unwrap().len(), 2);
    assert!(
        bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify that bola can now add caro as an admin
    bola_group
        .update_admin_list(UpdateAdminListType::Add, caro.inbox_id().to_string())
        .await
        .unwrap();
    bola_group.sync().await.unwrap();
    assert_eq!(bola_group.admin_list().unwrap().len(), 1);
    assert!(
        bola_group
            .admin_list()
            .unwrap()
            .contains(&caro.inbox_id().to_string())
    );

    // Verify that no one can remove a super admin from a group
    amal_group
        .remove_members_by_identity(&[bola.identifier()])
        .await
        .expect_err("expected err");

    // Verify that bola can now remove themself as a super admin
    bola_group
        .update_admin_list(
            UpdateAdminListType::RemoveSuper,
            bola.inbox_id().to_string(),
        )
        .await
        .unwrap();
    bola_group.sync().await.unwrap();
    assert_eq!(bola_group.super_admin_list().unwrap().len(), 1);
    assert!(
        !bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify that amal can NOT remove themself as a super admin because they are the only remaining
    amal_group
        .update_admin_list(
            UpdateAdminListType::RemoveSuper,
            amal.inbox_id().to_string(),
        )
        .await
        .expect_err("expected err");
}

#[xmtp_common::test]
async fn test_group_members_permission_level_update() {
    tester!(amal);
    tester!(bola);
    tester!(caro);

    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    // Add Bola and Caro to the group
    amal_group
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Initial checks for group members
    let initial_members = amal_group.members().await.unwrap();
    let mut count_member = 0;
    let mut count_admin = 0;
    let mut count_super_admin = 0;

    for member in &initial_members {
        match member.permission_level {
            PermissionLevel::Member => count_member += 1,
            PermissionLevel::Admin => count_admin += 1,
            PermissionLevel::SuperAdmin => count_super_admin += 1,
        }
    }

    assert_eq!(
        count_super_admin, 1,
        "Only Amal should be super admin initially"
    );
    assert_eq!(count_admin, 0, "no members are admin only");
    assert_eq!(count_member, 2, "two members have no admin status");

    // Add Bola as an admin
    amal_group
        .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Check after adding Bola as an admin
    let members = amal_group.members().await.unwrap();
    let mut count_member = 0;
    let mut count_admin = 0;
    let mut count_super_admin = 0;

    for member in &members {
        match member.permission_level {
            PermissionLevel::Member => count_member += 1,
            PermissionLevel::Admin => count_admin += 1,
            PermissionLevel::SuperAdmin => count_super_admin += 1,
        }
    }

    assert_eq!(
        count_super_admin, 1,
        "Only Amal should be super admin initially"
    );
    assert_eq!(count_admin, 1, "bola is admin");
    assert_eq!(count_member, 1, "caro has no admin status");

    // Add Caro as a super admin
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, caro.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Check after adding Caro as a super admin
    let members = amal_group.members().await.unwrap();
    let mut count_member = 0;
    let mut count_admin = 0;
    let mut count_super_admin = 0;

    for member in &members {
        match member.permission_level {
            PermissionLevel::Member => count_member += 1,
            PermissionLevel::Admin => count_admin += 1,
            PermissionLevel::SuperAdmin => count_super_admin += 1,
        }
    }

    assert_eq!(
        count_super_admin, 2,
        "Amal and Caro should be super admin initially"
    );
    assert_eq!(count_admin, 1, "bola is admin");
    assert_eq!(count_member, 0, "no members have no admin status");
}

#[xmtp_common::test]
async fn test_can_update_gce_after_failed_commit() {
    // Step 1: Amal creates a group
    tester!(amal);
    let policy_set = Some(PreconfiguredPolicies::Default.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    // Step 2:  Amal adds Bola to the group
    tester!(bola);
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Step 3: Verify that Bola can update the group name, and amal sees the update
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group: &TestMlsGroup = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    bola_group
        .update_group_name("Name Update 1".to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    let name = amal_group.group_name().unwrap();
    assert_eq!(name, "Name Update 1");

    // Step 4:  Bola attempts an action that they do not have permissions for like add admin, fails as expected
    let result = bola_group
        .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
        .await;
    if let Err(e) = &result {
        eprintln!("Error updating admin list: {:?}", e);
    }
    // Step 5: Now have Bola attempt to update the group name again
    bola_group
        .update_group_name("Name Update 2".to_string())
        .await
        .unwrap();

    // Step 6: Verify that both clients can sync without error and that the group name has been updated
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    let binding = amal_group.mutable_metadata().expect("msg");
    let amal_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "Name Update 2");
    let binding = bola_group.mutable_metadata().expect("msg");
    let bola_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(bola_group_name, "Name Update 2");
}

#[xmtp_common::test]
async fn test_can_update_permissions_after_group_creation() {
    tester!(amal);
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group: &TestMlsGroup = &amal.create_group(policy_set, None).unwrap();

    // Step 2:  Amal adds Bola to the group
    tester!(bola);
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Step 3: Bola attempts to add Caro, but fails because group is admin only
    tester!(caro);
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

    let bola_group: &TestMlsGroup = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let result = bola_group.add_members(&[caro.inbox_id()]).await;
    if let Err(e) = &result {
        eprintln!("Error adding member: {:?}", e);
    } else {
        panic!("Expected error adding member");
    }

    // Step 4: Bola attempts to update permissions but fails because they are not a super admin
    let result = bola_group
        .update_permission_policy(
            PermissionUpdateType::AddMember,
            PermissionPolicyOption::Allow,
            None,
        )
        .await;
    if let Err(e) = &result {
        eprintln!("Error updating permissions: {:?}", e);
    } else {
        panic!("Expected error updating permissions");
    }

    // Step 5: Amal updates group permissions so that all members can add
    amal_group
        .update_permission_policy(
            PermissionUpdateType::AddMember,
            PermissionPolicyOption::Allow,
            None,
        )
        .await
        .unwrap();

    // Step 6: Bola can now add Caro to the group
    bola_group.add_members(&[caro.inbox_id()]).await.unwrap();
    bola_group.sync().await.unwrap();
    let members = bola_group.members().await.unwrap();
    assert_eq!(members.len(), 3);
}

#[xmtp_common::test]
async fn test_respects_character_limits_for_group_metadata() {
    tester!(amal);

    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let overlong_name = "a".repeat(MAX_GROUP_NAME_LENGTH + 1);
    let overlong_description = "b".repeat(MAX_GROUP_DESCRIPTION_LENGTH + 1);
    let overlong_image_url =
        "http://example.com/".to_string() + &"c".repeat(MAX_GROUP_IMAGE_URL_LENGTH);

    // Verify that updating the name with an excessive length fails
    let result = amal_group.update_group_name(overlong_name).await;
    assert!(
        matches!(result, Err(GroupError::TooManyCharacters { length }) if length == MAX_GROUP_NAME_LENGTH)
    );

    // Verify that updating the description with an excessive length fails
    let result = amal_group
        .update_group_description(overlong_description)
        .await;
    assert!(
        matches!(result, Err(GroupError::TooManyCharacters { length }) if length == MAX_GROUP_DESCRIPTION_LENGTH)
    );

    // Verify that updating the image URL with an excessive length fails
    let result = amal_group
        .update_group_image_url_square(overlong_image_url)
        .await;
    assert!(
        matches!(result, Err(GroupError::TooManyCharacters { length }) if length == MAX_GROUP_IMAGE_URL_LENGTH)
    );

    // Verify that updating the app data with an excessive length fails
    let overlong_app_data = "d".repeat(MAX_APP_DATA_LENGTH + 1);
    let result = amal_group.update_app_data(overlong_app_data, None).await;
    assert!(
        matches!(result, Err(GroupError::TooManyCharacters { length }) if length == MAX_APP_DATA_LENGTH)
    );

    // Verify updates with valid lengths are successful
    let valid_name = "Valid Group Name".to_string();
    let valid_description = "Valid group description within limit.".to_string();
    let valid_image_url = "http://example.com/image.png".to_string();
    let valid_app_data = "Valid app data content".to_string();

    amal_group
        .update_group_name(valid_name.clone())
        .await
        .unwrap();
    amal_group
        .update_group_description(valid_description.clone())
        .await
        .unwrap();
    amal_group
        .update_group_image_url_square(valid_image_url.clone())
        .await
        .unwrap();
    amal_group
        .update_app_data(valid_app_data.clone(), None)
        .await
        .unwrap();

    // Sync and verify stored values
    amal_group.sync().await.unwrap();

    let metadata = amal_group.mutable_metadata().unwrap();

    assert_eq!(
        metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap(),
        &valid_name
    );
    assert_eq!(
        metadata
            .attributes
            .get(&MetadataField::Description.to_string())
            .unwrap(),
        &valid_description
    );
    assert_eq!(
        metadata
            .attributes
            .get(&MetadataField::GroupImageUrlSquare.to_string())
            .unwrap(),
        &valid_image_url
    );
    assert_eq!(
        metadata
            .attributes
            .get(&MetadataField::AppData.to_string())
            .unwrap(),
        &valid_app_data
    );
}
