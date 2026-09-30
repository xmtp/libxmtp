//! Map and set field deltas.

use super::*;

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
