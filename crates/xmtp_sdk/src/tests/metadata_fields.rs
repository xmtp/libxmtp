//! Metadata fields and user data through the SDK surface. Alix and Bo read
//! one group through different backend catalogues, so a name labels a
//! field for one reader only and the component ID identifies it.

use std::collections::HashMap;

use super::*;
use crate::{
    ApplicationComponentDefinition, ComponentMutation, ComponentPermissions, Conversation,
    ErrorCategory, FieldKey, FieldValue, Group, MapEntry, MapMutation, MetadataBasePolicy as Base,
    MetadataComponentType, MetadataFieldDescriptor, MetadataFieldRef, MetadataFieldValue,
    MetadataKeyType, MetadataPolicy, MetadataScalarType, MetadataValue, SetMutation,
    UserFieldUpdate, UserFieldValue, WellKnownMetadataField as WellKnown,
    metadata::conformance::use_application_components, metadata_field_ref,
};

const STATUS: u16 = 0xC001;
const NICKNAME: u16 = 0xC002;
const TOPIC: u16 = 0xC003;
const AVATAR: u16 = 0xC004;
const LATER: u16 = 0xC005;
const LABELS: u16 = 0xC006;
const TAGS: u16 = 0xC007;
const MEMBERS: u16 = 0xC008;
const KEPT: u16 = 0xC009;

const NICKNAME_TYPE: MetadataComponentType = MetadataComponentType::Map {
    key_type: MetadataKeyType::InboxId,
    value_type: MetadataScalarType::String,
};

fn field(component_id: u16, name: Option<&str>) -> MetadataFieldRef {
    MetadataFieldRef {
        component_id,
        name: name.map(Into::into),
    }
}

fn permissions(base: Base) -> ComponentPermissions {
    ComponentPermissions {
        insert: MetadataPolicy::Base(base),
        update: MetadataPolicy::Base(base),
        delete: MetadataPolicy::Base(base),
    }
}

fn definition(
    component_id: u16,
    name: &str,
    component_type: MetadataComponentType,
    base: Base,
    in_dms: bool,
) -> ApplicationComponentDefinition {
    ApplicationComponentDefinition {
        component_id,
        name: name.into(),
        component_type,
        permissions: permissions(base),
        in_groups: true,
        in_dms,
    }
}

/// Alix's catalogue. `later` has a type tag no SDK knows, so no
/// conversation registers it.
fn alix_catalogue() -> Vec<ApplicationComponentDefinition> {
    vec![
        definition(
            STATUS,
            "status",
            MetadataComponentType::String,
            Base::Allow,
            true,
        ),
        definition(
            NICKNAME,
            "nickname",
            NICKNAME_TYPE,
            Base::AllowIfSelfOrNonMember,
            true,
        ),
        definition(
            TOPIC,
            "topic",
            MetadataComponentType::String,
            Base::AllowIfAdmin,
            true,
        ),
        definition(
            AVATAR,
            "avatar",
            MetadataComponentType::Bytes,
            Base::Allow,
            false,
        ),
        definition(
            LATER,
            "later",
            MetadataComponentType::Unknown { tag: 99 },
            Base::Allow,
            true,
        ),
    ]
}

/// Bo's catalogue gives `status` to another field and names `STATUS`
/// after a well-known field, with a type and policy the group never
/// committed.
fn bo_catalogue() -> Vec<ApplicationComponentDefinition> {
    vec![
        definition(
            STATUS,
            "GROUP_NAME",
            MetadataComponentType::Bytes,
            Base::Deny,
            true,
        ),
        definition(
            AVATAR,
            "status",
            MetadataComponentType::Bytes,
            Base::Allow,
            false,
        ),
    ]
}

async fn client_with(catalogue: Vec<ApplicationComponentDefinition>) -> Client {
    use_application_components(Some(catalogue)).unwrap();
    let client = Client::create(crate::generate_local_signer().await, options()).await;
    use_application_components(None).unwrap();
    client.unwrap()
}

/// Alix's group with Bo, and Bo's handle on it.
async fn group_pair(alix: &Client, bo: &Client) -> (Arc<Group>, Arc<Group>) {
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await
        .unwrap();
    bo.conversations().sync().await.unwrap();
    let Some(Conversation::Group { group: bo_group }) =
        bo.conversations().get_by_id(group.id()).await.unwrap()
    else {
        panic!("Bo has the group");
    };
    (group, bo_group)
}

fn string(value: &str) -> FieldValue {
    FieldValue::String(value.into())
}

fn set(field: MetadataFieldRef, value: &str) -> UserFieldUpdate {
    UserFieldUpdate {
        field,
        value: Some(string(value)),
    }
}

fn user_value(field: MetadataFieldRef, value: &str) -> UserFieldValue {
    UserFieldValue {
        field,
        value: string(value),
    }
}

/// The error's variant name, code, category and retryability.
fn kind(error: XmtpError) -> (String, String, String, bool) {
    let name = format!("{error:?}");
    let name = name[..name.find('(').unwrap()].to_owned();
    let details = match error {
        XmtpError::UnknownField(details)
        | XmtpError::NotUserField(details)
        | XmtpError::DuplicateField(details)
        | XmtpError::TypeMismatch(details)
        | XmtpError::PermissionDenied(details)
        | XmtpError::InvalidArgument(details)
        | XmtpError::ClientClosed(details)
        | XmtpError::Unknown(details) => details,
        other => panic!("unexpected error {other:?}"),
    };
    let category = format!("{:?}", details.category);
    (name, details.code, category, details.retryable)
}

/// The error is the `name` variant with code `name` in `category`, and is
/// not retryable.
fn expect_kind(error: XmtpError, name: &str, category: ErrorCategory) {
    assert_eq!(
        kind(error),
        (name.into(), name.into(), format!("{category:?}"), false)
    );
}

/// Each reader lists the group's fields in component ID order, named by its
/// own catalogue, with the committed type and policies. A name finds the
/// reader's field, and a well-known name wins over a catalogue name.
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

/// A batch read answers in request order from one snapshot, with each
/// reader's label, and an absent value stays absent.
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

/// Absent filters select every user field and every member; an empty field
/// list gives each selected inbox an empty list, and an empty inbox list
/// selects no inbox. A DM reads its pair's profiles.
#[xmtp_common::test(unwrap_try = true)]
async fn user_data_keeps_absent_and_empty_filters_apart() {
    let alix = client_with(alix_catalogue()).await;
    let bo = client_with(bo_catalogue()).await;
    let (group, bo_group) = group_pair(&alix, &bo).await;
    let names = metadata_field_ref(WellKnown::UserDisplayName);
    let nickname = field(NICKNAME, Some("nickname"));
    bo_group
        .update_user_data(vec![
            set(names.clone(), "Bo"),
            set(field(NICKNAME, None), "B"),
        ])
        .await?;
    group.sync().await?;

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

    alix.end().await?;
    bo.end().await?;
}

/// A removal clears the removed inbox's user values that its remover may
/// delete and keeps the rest. Absent `inbox_ids` selects only current
/// members, so a kept value shows only when its inbox is named. A named
/// inbox appears once however often it is named, member or not.
#[xmtp_common::test(unwrap_try = true)]
async fn user_data_selects_named_inboxes_once() {
    let kept = ApplicationComponentDefinition {
        permissions: ComponentPermissions {
            delete: MetadataPolicy::Base(Base::Deny),
            ..permissions(Base::Allow)
        },
        ..definition(KEPT, "kept", NICKNAME_TYPE, Base::Allow, false)
    };
    let alix = client_with(vec![kept]).await;
    let bo = client_with(vec![]).await;
    let (group, bo_group) = group_pair(&alix, &bo).await;
    let names = metadata_field_ref(WellKnown::UserDisplayName);
    bo_group
        .update_user_data(vec![set(names, "Bo"), set(field(KEPT, None), "B")])
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

/// One call writes several of the caller's own fields in one commit. A
/// write that changes nothing makes no commit, a rejected batch commits
/// nothing, and a denied write is a typed `PermissionDenied`.
#[xmtp_common::test(unwrap_try = true)]
async fn profile_writes_are_atomic_and_denials_are_typed() {
    let alix = client_with(alix_catalogue()).await;
    let bo = client_with(bo_catalogue()).await;
    let (group, bo_group) = group_pair(&alix, &bo).await;
    let names = metadata_field_ref(WellKnown::UserDisplayName);

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

    alix.end().await?;
    bo.end().await?;
}

/// Map and set deltas apply in one commit or not at all, and reads return
/// their entries with bytes and inbox ID keys.
#[xmtp_common::test(unwrap_try = true)]
async fn collection_fields_apply_whole_deltas() {
    let alix = client_with(vec![
        definition(
            LABELS,
            "labels",
            MetadataComponentType::Map {
                key_type: MetadataKeyType::Bytes,
                value_type: MetadataScalarType::Bytes,
            },
            Base::Allow,
            false,
        ),
        definition(
            TAGS,
            "tags",
            MetadataComponentType::Set {
                key_type: MetadataKeyType::Bytes,
            },
            Base::Allow,
            false,
        ),
        definition(
            MEMBERS,
            "members",
            MetadataComponentType::Set {
                key_type: MetadataKeyType::InboxId,
            },
            Base::Allow,
            false,
        ),
    ])
    .await;
    let group = alix.conversations().create_group(vec![], None).await?;
    let (labels, tags, members) = (field(LABELS, None), field(TAGS, None), field(MEMBERS, None));
    let key = |text: &str| FieldKey::Bytes(text.into());
    let bytes = |byte: u8| FieldValue::Bytes(vec![byte]);
    let entry = |text: &str, byte: u8| MapEntry {
        key: key(text),
        value: bytes(byte),
    };

    group
        .update_metadata_field(
            labels.clone(),
            ComponentMutation::MapDelta(vec![
                MapMutation::Insert(key("a"), bytes(1)),
                MapMutation::Insert(key("b"), bytes(2)),
            ]),
        )
        .await?;
    group
        .update_metadata_field(
            labels.clone(),
            ComponentMutation::MapDelta(vec![
                MapMutation::Update(key("a"), bytes(3)),
                MapMutation::Delete(key("b")),
            ]),
        )
        .await?;
    // Deleting the absent `b` fails the whole delta, so `c` is not added.
    let epoch = group.inner.epoch().await?;
    expect_kind(
        group
            .update_metadata_field(
                labels.clone(),
                ComponentMutation::MapDelta(vec![
                    MapMutation::Insert(key("c"), bytes(4)),
                    MapMutation::Delete(key("b")),
                ]),
            )
            .await
            .unwrap_err(),
        "Unknown",
        ErrorCategory::Conversation,
    );
    assert_eq!(group.inner.epoch().await?, epoch);
    assert_eq!(
        group.metadata_value(labels.clone()).await?,
        Some(MetadataValue::Map(vec![entry("a", 3)]))
    );
    assert_eq!(
        group.map_value(labels.clone(), key("a")).await?,
        Some(bytes(3))
    );

    // A set key's hash is SHA-256 of its TLS encoding: a one-byte length
    // prefix, then the bytes.
    let x_hash = xmtp_cryptography::hash::sha256_array(&[1, b'x']).to_vec();
    group
        .update_metadata_field(
            tags.clone(),
            ComponentMutation::SetDelta(vec![
                SetMutation::Insert(key("x")),
                SetMutation::Insert(key("y")),
            ]),
        )
        .await?;
    group
        .update_metadata_field(
            tags.clone(),
            ComponentMutation::SetDelta(vec![SetMutation::DeleteByHash(x_hash)]),
        )
        .await?;
    assert_eq!(
        group.metadata_value(tags.clone()).await?,
        Some(MetadataValue::Set(vec![key("y")]))
    );
    expect_kind(
        group
            .update_metadata_field(
                tags.clone(),
                ComponentMutation::SetDelta(vec![SetMutation::DeleteByHash(vec![0; 31])]),
            )
            .await
            .unwrap_err(),
        "InvalidArgument",
        ErrorCategory::Input,
    );

    group
        .update_metadata_field(
            members.clone(),
            ComponentMutation::SetDelta(vec![SetMutation::Insert(FieldKey::InboxId(
                alix.inbox_id().into_checked()?,
            ))]),
        )
        .await?;
    assert_eq!(
        group.metadata_value(members).await?,
        Some(MetadataValue::Set(vec![FieldKey::InboxId(
            alix.inbox_id().into_checked()?
        )]))
    );

    group
        .update_metadata_field(labels.clone(), ComponentMutation::Remove)
        .await?;
    assert_eq!(group.metadata_value(labels).await?, None);

    alix.end().await?;
}

/// A metadata call on an ended client fails `ClientClosed`, for a read and
/// for a write.
#[xmtp_common::test(unwrap_try = true)]
async fn metadata_calls_after_end_are_closed() {
    let alix = client_with(alix_catalogue()).await;
    let group = alix.conversations().create_group(vec![], None).await?;
    alix.end().await?;

    expect_kind(
        group.metadata_fields().await.unwrap_err(),
        "ClientClosed",
        ErrorCategory::Lifecycle,
    );
    expect_kind(
        group
            .update_user_data(vec![set(field(NICKNAME, None), "A")])
            .await
            .unwrap_err(),
        "ClientClosed",
        ErrorCategory::Lifecycle,
    );
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
