//! Profile field tests: dictionary bounds.

use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn test_large_dictionary_membership_observation() {
    use xmtp_db::{encrypted_store::database::count_sql_queries, sql_key_store::count_kv_reads};

    const FIELDS: u16 = 16;
    let alix = EngineTestPeer::new_with(|o| {
        o.configure(|c| {
            c.application_components = (0..FIELDS)
                .map(|n| {
                    definition(
                        0xC110 + n,
                        ComponentType::TlsMapBytesBytes,
                        MetadataBasePolicy::Allow,
                        false,
                        true,
                    )
                })
                .collect()
        })
    })
    .await;
    let bo = EngineTestPeer::new().await;
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let bo_dm = bo.wait_for_welcomes().await?.pop()?;
    let fill = ComponentMutation::MapDelta(
        (0..8u8)
            .map(|k| {
                MapMutation::Insert(FieldKey::Bytes(vec![k]), FieldValue::Bytes(vec![k; 8000]))
            })
            .collect(),
    );
    dm.write_fields(|snapshot| {
        (0..FIELDS)
            .map(|n| {
                snapshot.field_write(&MetadataFieldRef::new(ComponentId::new(0xC110 + n)), &fill)
            })
            .collect()
    })
    .await?;
    bo_dm.sync().await?;
    assert!(
        dictionary(&bo_dm)?
            .entries()
            .map(|entry| entry.data().len())
            .sum::<usize>()
            >= usize::from(FIELDS) * 8 * 8000
    );

    for group in [&dm, &bo_dm] {
        let ((active, _, begins), reads) =
            count_kv_reads(|| count_sql_queries(|| group.is_active()));
        assert!(active?);
        assert_eq!(
            reads, 1,
            "membership observation loaded unrelated MLS state"
        );
        assert_eq!(begins, 1, "membership observation lost its writer snapshot");
    }

    dm.send_message(b"after large dictionary", Default::default())
        .await?;
    bo_dm.sync().await?;
    assert!(
        bo_dm
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .any(|message| message.decrypted_message_bytes == b"after large dictionary")
    );
}

/// A DM participant fills application maps until the next one would grow
/// the dictionary past 4 MiB: that write is refused as a field write and as a
/// payload intent, which fails without blocking the next intent. Nothing of
/// it is published, and the peer's dictionary is unchanged. A commit that
/// also empties a map is judged by the total it leaves.
// verifies: META-077
#[xmtp_common::test(unwrap_try = true)]
// Dozens of multi-MiB commits outlast the wasm job's time limit.
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_dictionary_growth_is_bounded() {
    const FIELDS: u16 = 67;
    let alix = EngineTestPeer::new_with(|o| {
        o.configure(|c| {
            c.application_components = (0..FIELDS)
                .map(|n| {
                    definition(
                        0xC110 + n,
                        ComponentType::TlsMapBytesBytes,
                        MetadataBasePolicy::Allow,
                        false,
                        true,
                    )
                })
                .collect()
        })
    })
    .await;
    let bo = EngineTestPeer::new().await;
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let bo_dm = bo.wait_for_welcomes().await?.pop()?;
    let total = |dictionary: AppDataDictionary| -> usize {
        let entries = dictionary.entries();
        let counted = entries.filter(|c| c.id() != ComponentId::GROUP_MEMBERSHIP.as_u16());
        counted.map(|c| c.data().len()).sum()
    };
    // Eight 8000-byte values: a map just under the snapshot bound.
    let fill = ComponentMutation::MapDelta(
        (0..8u8)
            .map(|k| {
                MapMutation::Insert(FieldKey::Bytes(vec![k]), FieldValue::Bytes(vec![k; 8000]))
            })
            .collect(),
    );
    let mut fields = (0..FIELDS).map(|n| MetadataFieldRef::new(ComponentId::new(0xC110 + n)));
    // Fill `count` maps in one commit, so the test reaches the limit in a
    // few commits.
    let mut fill_maps = |count: usize| {
        let batch: Vec<_> = fields.by_ref().take(count).collect();
        let fill = &fill;
        dm.write_fields(move |snapshot| {
            batch
                .iter()
                .map(|field| snapshot.field_write(field, fill))
                .collect()
        })
    };

    let before = total(dictionary(&dm)?);
    fill_maps(1).await?;
    let map = total(dictionary(&dm)?) - before;
    let mut room = (MAX_DICTIONARY_BYTES - before - map) / map;
    while room > 0 {
        // Each sync processes one commit, well inside the sync deadline.
        let count = room.min(16);
        fill_maps(count).await?;
        bo_dm.sync().await?;
        room -= count;
    }
    assert!(total(dictionary(&dm)?) + map > MAX_DICTIONARY_BYTES);
    bo_dm.sync().await?;
    let expected = dictionary(&bo_dm)?;
    assert_eq!(expected, dictionary(&dm)?);
    assert!(matches!(
        fill_maps(1).await,
        Err(GroupError::MetadataField(FieldError::Denied(_)))
    ));
    // The same map as a payload intent fails at once, without publishing,
    // and the message queued after it is still sent.
    let payload = (0..8u8).fold(TlsMapDelta::<VLBytes, VLBytes>::new(), |delta, k| {
        delta.insert(VLBytes::new(vec![k]), VLBytes::new(vec![k; 8000]))
    });
    let unused = ComponentId::new(0xC110 + FIELDS - 1);
    let payload = AppDataUpdateIntentData::new(unused.as_u16(), payload.tls_serialize_detached()?);
    let refused = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(payload))
        .queue(&dm)?;
    dm.send_message(b"after", Default::default()).await?;
    let failed =
        dm.context
            .db()
            .find_group_intents(dm.group_id, Some(vec![IntentState::Error]), None)?;
    assert!(failed.iter().any(|intent| intent.id == refused.id));
    bo_dm.sync().await?;
    assert_eq!(last_rejection(&bo_dm)?, None);
    assert_eq!(dictionary(&bo_dm)?, expected);
    let messages = bo_dm.find_messages(&MsgQueryArgs::default())?;
    assert!(
        messages
            .iter()
            .any(|m| m.decrypted_message_bytes == b"after")
    );

    // The same map is accepted when the commit also empties another, even
    // though the map comes first.
    let [unused, first] = [unused, ComponentId::new(0xC110)].map(MetadataFieldRef::new);
    let empty = (0..8u8).map(|k| MapMutation::Delete(FieldKey::Bytes(vec![k])));
    let empty = ComponentMutation::MapDelta(empty.collect());
    dm.write_fields(|snapshot| {
        let emptied = snapshot.field_write(&first, &empty)?;
        Ok(vec![snapshot.field_write(&unused, &fill)?, emptied])
    })
    .await?;
}
