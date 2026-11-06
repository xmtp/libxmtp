//! Profile field tests: dictionary bounds.

use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn test_large_dictionary_membership_observation() {
    use xmtp_db::{encrypted_store::database::count_sql_queries, sql_key_store::count_kv_reads};

    // About 448 KB: below half the default envelope, the most an honest
    // sender writes.
    const FIELDS: u16 = 7;
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

/// A DM member's writes stop at half the envelope, not at the receivers'
/// 4 MiB: a field write that grows the dictionary past it is refused and
/// publishes nothing, the same map as a payload intent fails without
/// blocking the next intent, a peer's standalone proposal that would grow it
/// past it stays out of the next commit, and a commit that also empties a
/// map is judged by the total it leaves.
// verifies: META-078
#[xmtp_common::test(unwrap_try = true)]
async fn the_sender_keeps_the_dictionary_within_half_an_envelope() {
    const ENVELOPE_LIMIT: usize = 256 * 1024;
    const BOUND: usize = ENVELOPE_LIMIT / 2;
    const FIELDS: u16 = 4;
    let catalogue: Vec<_> = (0..FIELDS)
        .map(|n| {
            definition(
                0xC110 + n,
                ComponentType::TlsMapBytesBytes,
                MetadataBasePolicy::Allow,
                false,
                true,
            )
        })
        .collect();
    let alix = EngineTestPeer::new_with(|o| {
        o.configure(|c| {
            c.application_components = catalogue.clone();
            c.limits.max_envelope_bytes = ENVELOPE_LIMIT;
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
    let fill = ComponentMutation::MapDelta(
        (0..8u8)
            .map(|k| {
                MapMutation::Insert(FieldKey::Bytes(vec![k]), FieldValue::Bytes(vec![k; 7000]))
            })
            .collect(),
    );
    let mut fields = (0..FIELDS).map(|n| MetadataFieldRef::new(ComponentId::new(0xC110 + n)));
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
    let room = (BOUND - before - map) / map;
    assert!(room > 0);
    fill_maps(room).await?;
    assert!(total(dictionary(&dm)?) + map > BOUND);
    bo_dm.sync().await?;
    let expected = dictionary(&bo_dm)?;
    assert_eq!(expected, dictionary(&dm)?);
    let epoch = dm.epoch().await?;
    assert!(matches!(
        fill_maps(1).await,
        Err(GroupError::MetadataField(FieldError::Denied(_)))
    ));
    assert_eq!(
        dm.epoch().await?,
        epoch,
        "the refused write publishes nothing"
    );

    // The same map as a payload intent fails at once, without publishing,
    // and the message queued after it is still sent.
    let unused = ComponentId::new(0xC110 + FIELDS - 1);
    let payload = (0..8u8).fold(TlsMapDelta::<VLBytes, VLBytes>::new(), |delta, k| {
        delta.insert(VLBytes::new(vec![k]), VLBytes::new(vec![k; 7000]))
    });
    let refused = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::new(
            unused.as_u16(),
            payload.tls_serialize_detached()?,
        )))
        .queue(&dm)?;
    dm.send_message(b"after the payload", Default::default())
        .await?;
    let failed =
        dm.context
            .db()
            .find_group_intents(dm.group_id, Some(vec![IntentState::Error]), None)?;
    assert!(failed.iter().any(|intent| intent.id == refused.id));
    assert_eq!(dm.epoch().await?, epoch, "the payload publishes nothing");

    // Bo, whose own bound is higher, proposes the same map alone. Alix's next
    // commit leaves it out, so neither side applies it.
    let proposal = state_write(bo.context.mls_storage(), |tx| {
        tx.with_group(bo_dm.group_id, |mls, storage| {
            let (message, _) = mls
                .propose_app_data_update(
                    &xmtp_db::XmtpOpenMlsProviderRef::new(storage),
                    &bo.context.identity().installation_keys,
                    unused.as_u16(),
                    AppDataUpdateOperation::Update(payload.tls_serialize_detached()?.into()),
                )
                .map_err(GroupError::Proposal)?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = bo_dm.prepare_group_messages(vec![(proposal.as_slice(), false)])?;
    bo.context.api().send_group_messages(messages).await?;
    dm.receive().await?;
    assert_eq!(
        dm.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        1
    );
    dm.send_message(b"after", Default::default()).await?;
    assert!(dm.epoch().await? > epoch, "the commit published");
    bo_dm.sync().await?;
    assert_eq!(bo_dm.epoch().await?, dm.epoch().await?);
    assert_eq!(last_rejection(&dm)?, None);
    assert_eq!(dictionary(&bo_dm)?, expected);
    assert_eq!(dictionary(&dm)?, expected);

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

/// A peer's standalone proposal that the builder would leave out under its
/// dictionary bound does not count against an own write: with it pending,
/// Alix's small insert is still accepted and published, and the proposal is
/// omitted.
// verifies: META-078
#[xmtp_common::test(unwrap_try = true)]
async fn a_proposal_the_builder_omits_does_not_refuse_an_own_write() {
    const ENVELOPE_LIMIT: usize = 256 * 1024;
    const FIELDS: u16 = 4;
    let catalogue: Vec<_> = (0..FIELDS)
        .map(|n| {
            definition(
                0xC110 + n,
                ComponentType::TlsMapBytesBytes,
                MetadataBasePolicy::Allow,
                false,
                true,
            )
        })
        .collect();
    let alix = EngineTestPeer::new_with(|o| {
        o.configure(|c| {
            c.application_components = catalogue.clone();
            c.limits.max_envelope_bytes = ENVELOPE_LIMIT;
        })
    })
    .await;
    let bo = EngineTestPeer::new().await;
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let bo_dm = bo.wait_for_welcomes().await?.pop()?;
    let field = |n: u16| MetadataFieldRef::new(ComponentId::new(0xC110 + n));
    // About 112 KB in two maps, under the 128 KiB bound.
    let fill = ComponentMutation::MapDelta(
        (0..8u8)
            .map(|k| {
                MapMutation::Insert(FieldKey::Bytes(vec![k]), FieldValue::Bytes(vec![k; 7000]))
            })
            .collect(),
    );
    dm.write_fields(|snapshot| {
        Ok(vec![
            snapshot.field_write(&field(0), &fill)?,
            snapshot.field_write(&field(1), &fill)?,
        ])
    })
    .await?;
    bo_dm.sync().await?;

    // Bo proposes a 56 KB map alone, which would cross Alix's bound.
    let unused = ComponentId::new(0xC110 + FIELDS - 1);
    let payload = (0..8u8).fold(TlsMapDelta::<VLBytes, VLBytes>::new(), |delta, k| {
        delta.insert(VLBytes::new(vec![k]), VLBytes::new(vec![k; 7000]))
    });
    let proposal = state_write(bo.context.mls_storage(), |tx| {
        tx.with_group(bo_dm.group_id, |mls, storage| {
            let (message, _) = mls
                .propose_app_data_update(
                    &xmtp_db::XmtpOpenMlsProviderRef::new(storage),
                    &bo.context.identity().installation_keys,
                    unused.as_u16(),
                    AppDataUpdateOperation::Update(payload.tls_serialize_detached()?.into()),
                )
                .map_err(GroupError::Proposal)?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = bo_dm.prepare_group_messages(vec![(proposal.as_slice(), false)])?;
    bo.context.api().send_group_messages(messages).await?;
    dm.receive().await?;
    assert_eq!(
        dm.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        1
    );

    let epoch = dm.epoch().await?;
    let insert = ComponentMutation::MapDelta(vec![MapMutation::Insert(
        FieldKey::Bytes(vec![9]),
        FieldValue::Bytes(vec![9; 4]),
    )]);
    dm.write_fields(|snapshot| Ok(vec![snapshot.field_write(&field(2), &insert)?]))
        .await?;
    assert!(dm.epoch().await? > epoch);
    let written = dictionary(&dm)?;
    assert!(
        written.get(&unused.as_u16()).is_none(),
        "the proposal was omitted"
    );
    assert!(written.get(&(0xC110 + 2)).is_some());
    bo_dm.sync().await?;
    assert_eq!(bo_dm.epoch().await?, dm.epoch().await?);
    assert_eq!(last_rejection(&dm)?, None);
}

/// Membership upkeep keeps to the builder's bound too. With the dictionary
/// just under half the envelope, Alix's client gains a catalogue component
/// that the group has not registered. Her next commit leaves that registry
/// insert out, because it would cross the bound, and still publishes.
// verifies: META-078
#[xmtp_common::test(unwrap_try = true)]
async fn upkeep_leaves_out_a_registry_insert_past_the_bound() {
    use crate::groups::tests::test_dictionary_creation::registry;
    const ENVELOPE_LIMIT: usize = 256 * 1024;
    const BOUND: usize = ENVELOPE_LIMIT / 2;
    let map = |n: u16| {
        definition(
            0xC110 + n,
            ComponentType::TlsMapBytesBytes,
            MetadataBasePolicy::Allow,
            true,
            true,
        )
    };
    let alix = EngineTestPeer::new_with(|o| {
        o.configure(|c| {
            c.application_components = vec![map(0), map(1), map(2)];
            c.limits.max_envelope_bytes = ENVELOPE_LIMIT;
        })
    })
    .await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let total = |dictionary: AppDataDictionary| -> usize {
        let entries = dictionary.entries();
        let counted = entries.filter(|c| c.id() != ComponentId::GROUP_MEMBERSHIP.as_u16());
        counted.map(|c| c.data().len()).sum()
    };
    let write = |n: u16, keys: std::ops::Range<u8>, len: usize| {
        let insert = ComponentMutation::MapDelta(
            keys.map(|key| {
                MapMutation::Insert(
                    FieldKey::Bytes(vec![key]),
                    FieldValue::Bytes(vec![key; len]),
                )
            })
            .collect(),
        );
        let group = &group;
        async move {
            group
                .write_fields(|snapshot| {
                    Ok(vec![snapshot.field_write(
                        &MetadataFieldRef::new(ComponentId::new(0xC110 + n)),
                        &insert,
                    )?])
                })
                .await
        }
    };
    // Fill the dictionary to a few bytes under the bound: two maps of
    // 8000-byte values, then smaller values in a third.
    write(0, 0..8, 8000).await?;
    write(1, 0..7, 8000).await?;
    let before = total(dictionary(&group)?);
    write(2, 0..1, 1).await?;
    let overhead = total(dictionary(&group)?) - before - 1;
    let mut key = 1;
    loop {
        let room = BOUND - total(dictionary(&group)?) - overhead;
        if room <= 4000 {
            write(2, key..key + 1, room - 4).await?;
            break;
        }
        write(2, key..key + 1, 4000).await?;
        key += 1;
    }
    let filled = total(dictionary(&group)?);
    assert!(filled <= BOUND && BOUND - filled < 16, "{filled}");
    bo_group.sync().await?;

    // The rebuilt client's catalogue has a component the group lacks.
    let alix = alix
        .with_options(|o| {
            o.configure(|c| {
                c.application_components = vec![map(0), map(1), map(2), map(3)];
                c.limits.max_envelope_bytes = ENVELOPE_LIMIT;
            })
        })
        .await;
    assert_eq!(
        alix.context.api().limits().max_envelope_bytes,
        ENVELOPE_LIMIT
    );
    let group = alix.group(&group.group_id)?;
    let epoch = group.epoch().await?;
    group.key_update().await?;
    assert!(group.epoch().await? > epoch, "the commit published");
    assert!(
        registry(&group)?.get(&ComponentId::new(0xC113))?.is_none(),
        "the registry insert was left out"
    );
    assert_eq!(total(dictionary(&group)?), filled);
    bo_group.sync().await?;
    assert_eq!(bo_group.epoch().await?, group.epoch().await?);
    assert_eq!(last_rejection(&group)?, None);
}

/// An intent's own metadata update keeps to the builder's bound too. With
/// the dictionary just under half the envelope, a group name that would
/// cross it is refused and publishes nothing, and the next message still
/// sends.
// verifies: META-078
#[xmtp_common::test(unwrap_try = true)]
async fn a_metadata_update_past_the_bound_is_refused() {
    use xmtp_mls_group_state::app_data::components::metadata_attributes::MAX_GROUP_NAME_LENGTH;
    const ENVELOPE_LIMIT: usize = 256 * 1024;
    const BOUND: usize = ENVELOPE_LIMIT / 2;
    let map = |n: u16| {
        definition(
            0xC110 + n,
            ComponentType::TlsMapBytesBytes,
            MetadataBasePolicy::Allow,
            true,
            true,
        )
    };
    let alix = EngineTestPeer::new_with(|o| {
        o.configure(|c| {
            c.application_components = vec![map(0), map(1), map(2)];
            c.limits.max_envelope_bytes = ENVELOPE_LIMIT;
        })
    })
    .await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let total = |dictionary: AppDataDictionary| -> usize {
        let entries = dictionary.entries();
        let counted = entries.filter(|c| c.id() != ComponentId::GROUP_MEMBERSHIP.as_u16());
        counted.map(|c| c.data().len()).sum()
    };
    let write = |n: u16, keys: std::ops::Range<u8>, len: usize| {
        let insert = ComponentMutation::MapDelta(
            keys.map(|key| {
                MapMutation::Insert(
                    FieldKey::Bytes(vec![key]),
                    FieldValue::Bytes(vec![key; len]),
                )
            })
            .collect(),
        );
        let group = &group;
        async move {
            group
                .write_fields(|snapshot| {
                    Ok(vec![snapshot.field_write(
                        &MetadataFieldRef::new(ComponentId::new(0xC110 + n)),
                        &insert,
                    )?])
                })
                .await
        }
    };
    // Fill the dictionary to a few bytes under the bound, as above.
    write(0, 0..8, 8000).await?;
    write(1, 0..7, 8000).await?;
    let before = total(dictionary(&group)?);
    write(2, 0..1, 1).await?;
    let overhead = total(dictionary(&group)?) - before - 1;
    let mut key = 1;
    loop {
        let room = BOUND - total(dictionary(&group)?) - overhead;
        if room <= 4000 {
            write(2, key..key + 1, room - 4).await?;
            break;
        }
        write(2, key..key + 1, 4000).await?;
        key += 1;
    }
    let filled = total(dictionary(&group)?);
    assert!(filled <= BOUND && BOUND - filled < 16, "{filled}");
    let name = group.group_name()?;
    assert!(name.len() + 16 < MAX_GROUP_NAME_LENGTH);

    let epoch = group.epoch().await?;
    assert!(
        group
            .update_group_name("n".repeat(MAX_GROUP_NAME_LENGTH))
            .await
            .is_err()
    );
    assert_eq!(
        group.epoch().await?,
        epoch,
        "the refused update publishes nothing"
    );
    assert_eq!(group.group_name()?, name);
    assert_eq!(total(dictionary(&group)?), filled);
    group
        .send_message(b"after the name", Default::default())
        .await?;
    bo_group.sync().await?;
    assert_eq!(bo_group.epoch().await?, group.epoch().await?);
    assert_eq!(last_rejection(&group)?, None);
}
