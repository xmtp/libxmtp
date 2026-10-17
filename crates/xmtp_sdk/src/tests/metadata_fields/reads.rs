//! Batch reads of field values.

use super::*;

/// A batch read answers in request order from one snapshot, with each
/// reader's label, and an absent value stays absent.
// verifies: META-070, META-071
#[xmtp_common::test(unwrap_try = true)]
async fn batch_reads_keep_request_order() {
    let alix = client_with(alix_catalogue()).await;
    let bo = client_with(bo_catalogue()).await;
    let (group, bo_group) = group_pair(&alix, &bo).await;
    let group_name = metadata_field_ref(WellKnown::GroupName);

    // Bo's catalogue calls STATUS a denied bytes field; the group's
    // registry makes it an open string field.
    bo_group
        .update_metadata_field(
            field(STATUS, None),
            ComponentMutation::Replace(string("hello")),
        )
        .await?;
    group.sync().await?;
    group
        .update_metadata_field(
            field(AVATAR, Some("avatar")),
            ComponentMutation::Replace(FieldValue::Bytes(vec![1, 2, 3])),
        )
        .await?;
    group
        .update_metadata_field(
            group_name.clone(),
            ComponentMutation::Replace(string("Team")),
        )
        .await?;
    bo_group.sync().await?;

    let scalar = |value| Some(MetadataValue::Scalar(value));
    assert_eq!(
        bo_group
            .metadata_values(vec![
                field(AVATAR, Some("status")),
                field(STATUS, Some("status")),
                group_name.clone(),
                field(TOPIC, None),
            ])
            .await?,
        [
            MetadataFieldValue {
                field: field(AVATAR, Some("status")),
                value: scalar(FieldValue::Bytes(vec![1, 2, 3])),
            },
            MetadataFieldValue {
                field: field(STATUS, Some("GROUP_NAME")),
                value: scalar(string("hello")),
            },
            MetadataFieldValue {
                field: group_name.clone(),
                value: scalar(string("Team")),
            },
            MetadataFieldValue {
                field: field(TOPIC, None),
                value: None,
            },
        ]
    );
    assert_eq!(bo_group.metadata_values(vec![]).await?, []);
    assert_eq!(bo_group.metadata_value(field(TOPIC, None)).await?, None);
    assert_eq!(
        bo_group.metadata_value(field(STATUS, None)).await?,
        scalar(string("hello"))
    );

    bo_group
        .update_user_data(vec![set(field(NICKNAME, None), "B")])
        .await?;
    group.sync().await?;
    let bo_key = FieldKey::InboxId(bo.inbox_id().into_checked()?);
    assert_eq!(
        group.map_value(field(NICKNAME, None), bo_key).await?,
        Some(string("B"))
    );
    assert_eq!(
        group
            .map_value(
                field(NICKNAME, None),
                FieldKey::InboxId(alix.inbox_id().into_checked()?)
            )
            .await?,
        None
    );

    expect_kind(
        group.metadata_value(field(0xC0FF, None)).await.unwrap_err(),
        "UnknownField",
        ErrorCategory::Input,
    );
    expect_kind(
        group
            .update_metadata_field(
                field(STATUS, None),
                ComponentMutation::Replace(FieldValue::Bytes(vec![1])),
            )
            .await
            .unwrap_err(),
        "TypeMismatch",
        ErrorCategory::Input,
    );
    expect_kind(
        group
            .map_value(field(NICKNAME, None), FieldKey::InboxId("not hex".into()))
            .await
            .unwrap_err(),
        "InvalidArgument",
        ErrorCategory::Input,
    );

    alix.end().await?;
    bo.end().await?;
}
