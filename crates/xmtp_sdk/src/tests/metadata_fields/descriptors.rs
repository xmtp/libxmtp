//! Field descriptors and name lookup.

use super::*;

/// Each reader lists the group's fields in component ID order, named by its
/// own catalogue, with the committed type and policies. A name finds the
/// reader's field, and a well-known name wins over a catalogue name.
// verifies: META-069
#[xmtp_common::test(unwrap_try = true)]
async fn fields_are_identified_by_component_id() {
    let alix = client_with(alix_catalogue()).await;
    let bo = client_with(bo_catalogue()).await;
    assert_eq!(
        alix.server_configuration().application_components,
        alix_catalogue()
    );
    let (group, bo_group) = group_pair(&alix, &bo).await;

    let well_known: Vec<_> = [
        WellKnown::GroupName,
        WellKnown::GroupDescription,
        WellKnown::GroupImageUrl,
        WellKnown::MessageDisappearFromNs,
        WellKnown::MessageDisappearInNs,
        WellKnown::AppData,
        WellKnown::UserDisplayName,
        WellKnown::GroupImage,
    ]
    .into_iter()
    .map(|known| {
        let field = metadata_field_ref(known);
        let user = matches!(known, WellKnown::UserDisplayName);
        (field, user)
    })
    .collect();
    let described = |fields: Vec<MetadataFieldDescriptor>| -> Vec<_> {
        fields
            .into_iter()
            .map(|descriptor| (descriptor.field, descriptor.is_user_field))
            .collect()
    };
    let fields = group.metadata_fields().await?;
    assert_eq!(described(fields.clone())[..8], well_known);
    let application = |labels: [Option<&str>; 4]| {
        [
            (STATUS, MetadataComponentType::String, Base::Allow, false),
            (NICKNAME, NICKNAME_TYPE, Base::AllowIfSelfOrNonMember, true),
            (
                TOPIC,
                MetadataComponentType::String,
                Base::AllowIfAdmin,
                false,
            ),
            (AVATAR, MetadataComponentType::Bytes, Base::Allow, false),
        ]
        .into_iter()
        .zip(labels)
        .map(
            |((id, component_type, base, is_user_field), name)| MetadataFieldDescriptor {
                field: field(id, name),
                component_type,
                permissions: permissions(base),
                is_user_field,
            },
        )
        .collect::<Vec<_>>()
    };
    assert_eq!(
        fields[8..],
        application([
            Some("status"),
            Some("nickname"),
            Some("topic"),
            Some("avatar")
        ])
    );

    // Bo's labels come from Bo's catalogue; the type and policies are the
    // group's.
    let bo_fields = bo_group.metadata_fields().await?;
    assert_eq!(described(bo_fields.clone())[..8], well_known);
    assert_eq!(
        bo_fields[8..],
        application([Some("GROUP_NAME"), None, None, Some("status")])
    );

    let found = |descriptor: Option<MetadataFieldDescriptor>| descriptor.map(|d| d.field);
    assert_eq!(
        found(group.metadata_field("status".into()).await?),
        Some(field(STATUS, Some("status")))
    );
    assert_eq!(
        found(bo_group.metadata_field("status".into()).await?),
        Some(field(AVATAR, Some("status")))
    );
    assert_eq!(
        found(bo_group.metadata_field("GROUP_NAME".into()).await?),
        Some(metadata_field_ref(WellKnown::GroupName))
    );
    assert_eq!(found(group.metadata_field("later".into()).await?), None);

    // A DM lists the pair's profile and the fields registered for DMs.
    let dm = alix.conversations().create_dm(bo.inbox_id(), None).await?;
    let dm_ids: Vec<_> = dm
        .metadata_fields()
        .await?
        .into_iter()
        .map(|descriptor| descriptor.field.component_id)
        .collect();
    assert!(dm_ids.contains(&0x800C));
    assert!(dm_ids.contains(&STATUS) && dm_ids.contains(&NICKNAME));
    assert!(!dm_ids.contains(&AVATAR));
    expect_kind(
        dm.update_metadata_field(
            field(AVATAR, None),
            ComponentMutation::Replace(FieldValue::Bytes(vec![1])),
        )
        .await
        .unwrap_err(),
        "UnknownField",
        ErrorCategory::Input,
    );

    alix.end().await?;
    bo.end().await?;
}
