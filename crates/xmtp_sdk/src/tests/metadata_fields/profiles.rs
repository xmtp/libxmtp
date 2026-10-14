//! User data reads and profile writes.

use super::*;

/// Absent filters select every user field and every member; an empty field
/// list gives each selected inbox an empty list, and an empty inbox list
/// selects no inbox. A DM reads its pair's profiles.
// verifies: META-072, META-073
#[xmtp_common::test(unwrap_try = true)]
async fn user_data_keeps_absent_and_empty_filters_apart() {
    let kept = ApplicationComponentDefinition {
        permissions: ComponentPermissions {
            delete: MetadataPolicy::Base(Base::Deny),
            ..permissions(Base::Allow)
        },
        ..definition(KEPT, "kept", NICKNAME_TYPE, Base::Allow, false)
    };
    let mut catalogue = alix_catalogue();
    catalogue.push(kept);
    let alix = client_with(catalogue).await;
    let bo = client_with(bo_catalogue()).await;
    let (group, bo_group) = group_pair(&alix, &bo).await;
    let names = metadata_field_ref(WellKnown::UserDisplayName);
    let nickname = field(NICKNAME, Some("nickname"));
    let epoch = bo_group.inner.epoch().await?;
    bo_group
        .update_user_data(vec![
            set(names.clone(), "Bo"),
            set(field(NICKNAME, None), "B"),
        ])
        .await?;
    assert_eq!(bo_group.inner.epoch().await?, epoch + 1);
    group.sync().await?;
    assert_eq!(
        group.user_data(None, Some(vec![bo.inbox_id()])).await?[&bo.inbox_id()].len(),
        2
    );
    bo_group.update_user_data(vec![]).await?;
    bo_group
        .update_user_data(vec![set(names.clone(), "Bo")])
        .await?;
    alix_clear_absent(&group).await;
    assert_eq!(bo_group.inner.epoch().await?, epoch + 1);
    expect_kind(
        bo_group
            .update_user_data(vec![
                set(names.clone(), "Bobby"),
                UserFieldUpdate {
                    field: names.clone(),
                    value: None,
                },
            ])
            .await
            .unwrap_err(),
        "DuplicateField",
        ErrorCategory::Input,
    );
    expect_kind(
        bo_group
            .update_metadata_field(field(TOPIC, None), ComponentMutation::Replace(string("x")))
            .await
            .unwrap_err(),
        "PermissionDenied",
        ErrorCategory::Conversation,
    );
    assert_eq!(bo_group.inner.epoch().await?, epoch + 1);
    assert_eq!(bo_group.metadata_value(field(TOPIC, None)).await?, None);
    assert_eq!(
        bo_group
            .map_value(
                names.clone(),
                FieldKey::InboxId(bo.inbox_id().into_checked()?)
            )
            .await?,
        Some(string("Bo"))
    );

    assert_eq!(
        group.user_data(None, None).await?,
        HashMap::from([
            (alix.inbox_id(), vec![]),
            (
                bo.inbox_id(),
                vec![
                    user_value(names.clone(), "Bo"),
                    user_value(nickname.clone(), "B")
                ]
            ),
        ])
    );
    assert_eq!(
        group.user_data(Some(vec![]), None).await?,
        HashMap::from([(alix.inbox_id(), vec![]), (bo.inbox_id(), vec![])])
    );
    assert_eq!(group.user_data(None, Some(vec![])).await?, HashMap::new());
    assert_eq!(
        group
            .user_data(Some(vec![field(NICKNAME, None)]), Some(vec![bo.inbox_id()]))
            .await?,
        HashMap::from([(bo.inbox_id(), vec![user_value(nickname, "B")])])
    );
    expect_kind(
        group
            .user_data(Some(vec![field(STATUS, None)]), None)
            .await
            .unwrap_err(),
        "NotUserField",
        ErrorCategory::Input,
    );

    let dm = alix.conversations().create_dm(bo.inbox_id(), None).await?;
    dm.update_user_data(vec![set(names.clone(), "Alix")])
        .await?;
    assert_eq!(
        dm.user_data(None, None).await?,
        HashMap::from([
            (alix.inbox_id(), vec![user_value(names, "Alix")]),
            (bo.inbox_id(), vec![]),
        ])
    );

    bo_group
        .update_user_data(vec![set(field(KEPT, None), "B")])
        .await?;
    group.sync().await?;
    group.remove_members(vec![bo.inbox_id()]).await?;
    assert_eq!(
        group.user_data(None, None).await?,
        HashMap::from([(alix.inbox_id(), vec![])])
    );
    let stranger = InboxId::try_from("ab".repeat(32))?;
    assert_eq!(
        group
            .user_data(
                None,
                Some(vec![bo.inbox_id(), stranger.clone(), bo.inbox_id()])
            )
            .await?,
        HashMap::from([
            (
                bo.inbox_id(),
                vec![user_value(field(KEPT, Some("kept")), "B")]
            ),
            (stranger, vec![]),
        ])
    );

    alix.end().await?;
    bo.end().await?;
}

/// Clearing an entry Alix never set changes nothing.
async fn alix_clear_absent(group: &Group) {
    let epoch = group.inner.epoch().await.unwrap();
    group
        .update_user_data(vec![UserFieldUpdate {
            field: field(NICKNAME, None),
            value: None,
        }])
        .await
        .unwrap();
    assert_eq!(group.inner.epoch().await.unwrap(), epoch);
}
