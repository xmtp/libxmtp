use super::*;
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind, RecoveryPosition};

#[xmtp_common::test(unwrap_try = true)]
fn recovery_visibility_filter_moves_raw_bound_buffers() {
    let query = RecoveryQueryArgs {
        messages: MsgQueryArgs {
            kind: Some(GroupMessageKind::MembershipChange),
            delivery_status: Some(DeliveryStatus::Failed),
            limit: Some(50),
            exclude_content_types: Some(vec![ContentType::Markdown, ContentType::Reaction]),
            ..Default::default()
        },
        before: Some(RecoveryPosition {
            sent_at_ns: -10,
            database_id: [1; 16],
            message_id: vec![3; 1024 * 1024],
        }),
        after: Some(RecoveryPosition {
            sent_at_ns: 20,
            database_id: [2; 16],
            message_id: vec![4; 1024 * 1024 + 1],
        }),
    };
    let buffers = [&query.before, &query.after].map(|position| {
        let position = position.as_ref().unwrap();
        (
            position.message_id.as_ptr(),
            position.message_id.capacity(),
            position.sent_at_ns,
            position.database_id,
            position.message_id.len(),
            position.message_id[0],
        )
    });
    let filtered = visible_recovery_query(query);
    for (position, expected) in [&filtered.before, &filtered.after].into_iter().zip(buffers) {
        let position = position.as_ref().unwrap();
        assert_eq!(position.message_id.as_ptr(), expected.0);
        assert_eq!(position.message_id.capacity(), expected.1);
        assert_eq!(position.sent_at_ns, expected.2);
        assert_eq!(position.database_id, expected.3);
        assert_eq!(position.message_id.len(), expected.4);
        assert_eq!(position.message_id[0], expected.5);
    }
    assert_eq!(
        filtered.messages.kind,
        Some(GroupMessageKind::MembershipChange)
    );
    assert_eq!(
        filtered.messages.delivery_status,
        Some(DeliveryStatus::Failed)
    );
    assert_eq!(filtered.messages.limit, Some(50));
    assert_eq!(
        filtered.messages.exclude_content_types,
        Some(vec![
            ContentType::Markdown,
            ContentType::Reaction,
            ContentType::ReadReceipt,
            ContentType::DeleteMessage,
        ])
    );
}

#[cfg(not(target_arch = "wasm32"))]
mod snapshot {
    use super::*;
    use crate::groups::QueryableContentFields;
    use crate::messages::decoded_message::MessageBody;
    use xmtp_common::Generate;
    use xmtp_content_types::{ContentCodec, test_utils::TestContentGenerator, text::TextCodec};
    use xmtp_db::{
        ConnectionExt, Store, TestDb, XmtpTestDb,
        diesel::{RunQueryDsl, prelude::*},
        group::{ConversationType, GroupMembershipState, StoredGroup},
        group_message::{SortBy, StoredGroupMessage},
        message_deletion::StoredMessageDeletion,
    };
    use xmtp_proto::{
        types::GroupId,
        xmtp::mls::message_contents::{EncodedContent, content_types::ReactionAction},
    };

    fn message(
        group_id: GroupId,
        id: u8,
        sent_at_ns: i64,
        delivery_status: DeliveryStatus,
        content: EncodedContent,
    ) -> StoredGroupMessage {
        let fields = QueryableContentFields::try_from(content.clone()).unwrap();
        StoredGroupMessage {
            id: vec![id; 32],
            group_id,
            decrypted_message_bytes: xmtp_content_types::encoded_content_to_bytes(content),
            sent_at_ns,
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1; 32],
            sender_inbox_id: "sender".into(),
            delivery_status,
            content_type: fields.content_type,
            version_major: fields.version_major,
            version_minor: fields.version_minor,
            authority_id: fields.authority_id,
            reference_id: fields.reference_id,
            sequence_id: 0,
            envelope_hash: None,
            expiry_ns: None,
            inserted_at_ns: 0,
            expiry_unknown: false,
            expire_at_ns: None,
            should_push: false,
            idempotency_key: id.to_string(),
        }
    }

    fn assert_relations(messages: &[EnrichedStoredMessage], later: bool) {
        let row = |id: u8| {
            messages
                .iter()
                .find(|row| row.stored.id == vec![id; 32])
                .unwrap()
        };
        assert_eq!(
            matches!(row(2).decoded.content, MessageBody::DeletedMessage { .. }),
            later,
            "deletion must use the page snapshot"
        );
        assert_eq!(row(3).decoded.reactions.len(), usize::from(later));
        assert_eq!(row(3).decoded.num_replies, usize::from(later));
        let MessageBody::Reply(reply) = &row(4).decoded.content else {
            panic!("expected reply");
        };
        let parent = reply.in_reply_to.as_ref().unwrap();
        let MessageBody::Text(text) = &parent.content else {
            panic!("expected text parent");
        };
        assert_eq!(
            text.content,
            if later { "new parent" } else { "old parent" }
        );
    }

    async fn relation_snapshot(recovery: bool) {
        for stitched in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let path = home.path().join("relations.db").display().to_string();
            let reader = TestDb::create_persistent_store(Some(path.clone())).await;
            let writer = TestDb::create_persistent_store(Some(path)).await;
            let db = reader.db();
            #[derive(QueryableByName)]
            struct JournalMode {
                #[diesel(sql_type = xmtp_db::diesel::sql_types::Text)]
                journal_mode: String,
            }
            assert_eq!(
                db.raw_query(|sqlite| xmtp_db::diesel::sql_query("PRAGMA journal_mode")
                    .get_result::<JournalMode>(sqlite))
                    .unwrap()
                    .journal_mode,
                "wal"
            );
            let mut groups = [0, 1].map(|_| {
                StoredGroup::builder()
                    .id(GroupId::generate())
                    .created_at_ns(0)
                    .membership_state(GroupMembershipState::Allowed)
                    .added_by_inbox_id("sender")
                    .build()
                    .unwrap()
            });
            for group in &mut groups {
                if stitched {
                    group.conversation_type = ConversationType::Dm;
                    group.dm_id = Some("relation-snapshot".into());
                }
                group.store(&db).unwrap();
            }
            let source = groups[usize::from(stitched)].id;
            let status = if recovery {
                DeliveryStatus::Unpublished
            } else {
                DeliveryStatus::Published
            };
            let parent = message(
                source,
                1,
                1,
                status,
                TestContentGenerator::text_content("old parent"),
            );
            let victim = message(
                source,
                2,
                10,
                status,
                TestContentGenerator::text_content("victim"),
            );
            let target = message(
                source,
                3,
                11,
                status,
                TestContentGenerator::text_content("target"),
            );
            let reply = message(
                source,
                4,
                12,
                status,
                TestContentGenerator::reply_content(
                    &hex::encode(&parent.id),
                    TextCodec::content_type(),
                    b"reply".to_vec(),
                ),
            );
            for row in [&parent, &victim, &target, &reply] {
                row.store(&db).unwrap();
            }
            if stitched {
                message(
                    groups[0].id,
                    5,
                    13,
                    status,
                    TestContentGenerator::text_content("other source"),
                )
                .store(&db)
                .unwrap();
            }
            let query = MsgQueryArgs {
                sent_after_ns: Some(9),
                sent_before_ns: Some(15),
                limit: Some(50),
                sort_by: Some(if recovery {
                    SortBy::SentAt
                } else {
                    SortBy::SentAtDelivery
                }),
                ..Default::default()
            };
            let original_cursor = db.current_delivery_cursor().unwrap();
            let committed = std::rc::Rc::new(std::cell::Cell::new(false));
            let observed = committed.clone();
            let writer_db = writer.db();
            BEFORE_ENRICHMENT.with_borrow_mut(|hook| {
                *hook =
                    Some(Box::new(move || {
                        writer_db
                            .raw_query(|sqlite| {
                                Ok(sqlite.immediate_transaction::<_, EnrichMessageError, _>(
                                    |sqlite| {
                                        let store = sqlite.key_store();
                                        let db = store.db();
                                        message(
                                            source,
                                            6,
                                            20,
                                            status,
                                            TestContentGenerator::reaction_content(
                                                &hex::encode(&target.id),
                                                "👍",
                                                ReactionAction::Added,
                                            ),
                                        )
                                        .store(&db)?;
                                        message(
                                            source,
                                            7,
                                            21,
                                            status,
                                            TestContentGenerator::reply_content(
                                                &hex::encode(&target.id),
                                                TextCodec::content_type(),
                                                b"later reply".to_vec(),
                                            ),
                                        )
                                        .store(&db)?;
                                        message(
                                            source,
                                            8,
                                            22,
                                            status,
                                            TestContentGenerator::delete_message_content(
                                                &hex::encode(&victim.id),
                                            ),
                                        )
                                        .store(&db)?;
                                        StoredMessageDeletion {
                                            id: vec![8; 32],
                                            group_id: source,
                                            deleted_message_id: victim.id,
                                            deleted_by_inbox_id: victim.sender_inbox_id,
                                            is_super_admin_deletion: false,
                                            deleted_at_ns: 22,
                                        }
                                        .store(&db)?;
                                        use xmtp_db::schema::group_messages::dsl;
                                        db.raw_query(|sqlite| {
                                            xmtp_db::diesel::update(
                                                dsl::group_messages.filter(dsl::id.eq(&parent.id)),
                                            )
                                            .set(dsl::decrypted_message_bytes.eq(
                                                xmtp_content_types::encoded_content_to_bytes(
                                                    TestContentGenerator::text_content(
                                                        "new parent",
                                                    ),
                                                ),
                                            ))
                                            .execute(sqlite)
                                        })?;
                                        Ok(())
                                    },
                                ))
                            })
                            .unwrap()
                            .unwrap();
                        observed.set(true);
                    }));
            });
            if recovery {
                let read = || {
                    recovery_page_with_stored(
                        &db,
                        &groups[0].id,
                        RecoveryQueryArgs {
                            messages: query.clone(),
                            ..Default::default()
                        },
                    )
                    .unwrap()
                };
                let page = read();
                assert!(
                    committed.get(),
                    "the WAL writer must commit before enrichment"
                );
                assert_relations(&page.messages, false);
                assert_eq!(page.messages.len(), if stitched { 4 } else { 3 });
                assert_eq!(page.first_position.as_ref().unwrap().sent_at_ns, 10);
                assert_eq!(
                    page.last_position.as_ref().unwrap().sent_at_ns,
                    if stitched { 13 } else { 12 }
                );
                assert!(!page.has_more);
                let later = read();
                assert_relations(&later.messages, true);
                assert_eq!(page.first_position, later.first_position);
                assert_eq!(page.last_position, later.last_position);
                assert_eq!(db.current_delivery_cursor().unwrap(), original_cursor);
            } else {
                let page = history_page_with_stored(&db, &groups[0].id, &query).unwrap();
                assert!(
                    committed.get(),
                    "the WAL writer must commit before enrichment"
                );
                assert_relations(&page.messages, false);
                assert_eq!(page.messages.len(), if stitched { 4 } else { 3 });
                assert_eq!(page.first_position.as_ref().unwrap().sent_at_ns, 10);
                assert_eq!(
                    page.last_position.as_ref().unwrap().sent_at_ns,
                    if stitched { 13 } else { 12 }
                );
                assert!(!page.has_more);
                let later = history_page_with_stored(&db, &groups[0].id, &query).unwrap();
                assert_relations(&later.messages, true);
                assert_eq!(page.first_position, later.first_position);
                assert_eq!(page.last_position, later.last_position);
            }
        }
    }

    #[xmtp_common::test]
    async fn history_page_relation_snapshot() {
        relation_snapshot(false).await;
    }

    #[xmtp_common::test]
    async fn recovery_page_relation_snapshot() {
        relation_snapshot(true).await;
    }

    async fn large_relation_lists(recovery: bool) {
        // Selected IDs plus repeated parent IDs exceed the native SQLite bind limit.
        const REPLY_COUNT: usize = 16_384;
        for stitched in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let store = TestDb::create_persistent_store(Some(
                home.path().join("large-relations.db").display().to_string(),
            ))
            .await;
            let db = store.db();
            #[derive(QueryableByName)]
            struct CompileOption {
                #[diesel(sql_type = xmtp_db::diesel::sql_types::Text)]
                option: String,
            }
            assert_eq!(
                db.raw_query(|sqlite| xmtp_db::diesel::sql_query(
                    "SELECT compile_options AS option FROM pragma_compile_options \
                 WHERE compile_options LIKE 'MAX_VARIABLE_NUMBER=%'"
                )
                .get_result::<CompileOption>(sqlite))
                    .unwrap()
                    .option,
                "MAX_VARIABLE_NUMBER=32766"
            );
            let groups = [0, 1].map(|_| {
                StoredGroup::builder()
                    .id(GroupId::generate())
                    .created_at_ns(0)
                    .membership_state(GroupMembershipState::Allowed)
                    .added_by_inbox_id("sender")
                    .conversation_type(if stitched {
                        ConversationType::Dm
                    } else {
                        ConversationType::Group
                    })
                    .dm_id(stitched.then(|| "large-relations".to_string()))
                    .build()
                    .unwrap()
            });
            for group in &groups {
                group.store(&db).unwrap();
            }
            let source = groups[usize::from(stitched)].id;
            let status = if recovery {
                DeliveryStatus::Unpublished
            } else {
                DeliveryStatus::Published
            };
            for (id, time, text) in [
                (1, 1, "alive parent"),
                (2, 1, "deleted parent"),
                (3, 11, "target"),
                (4, 12, "deleted row"),
            ] {
                message(
                    source,
                    id,
                    time,
                    status,
                    TestContentGenerator::text_content(text),
                )
                .store(&db)
                .unwrap();
            }
            if stitched {
                message(
                    groups[0].id,
                    5,
                    10,
                    status,
                    TestContentGenerator::text_content("other source"),
                )
                .store(&db)
                .unwrap();
            }
            let reply_content = |id: u8| {
                TestContentGenerator::reply_content(
                    &hex::encode(vec![id; 32]),
                    TextCodec::content_type(),
                    b"reply".to_vec(),
                )
            };
            let alive_reply = xmtp_content_types::encoded_content_to_bytes(reply_content(1));
            let deleted_reply = xmtp_content_types::encoded_content_to_bytes(reply_content(2));
            use xmtp_db::diesel::sql_types::{BigInt, Binary, Bool, Integer};
            db.raw_query(|sqlite| xmtp_db::diesel::sql_query(
                "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<?) \
                 INSERT INTO group_messages (id,group_id,decrypted_message_bytes,sent_at_ns,kind,\
                 sender_installation_id,sender_inbox_id,delivery_status,content_type,version_major,\
                 version_minor,authority_id,reference_id,sequence_id,delivery_sequence,idempotency_key) \
                 SELECT CAST(printf('%032d',x) AS BLOB), ?, CASE WHEN x%2=1 THEN ? ELSE ? END,\
                 13,1,x'01','sender',?, ?,1,0,'xmtp.org',CASE WHEN x%2=1 THEN ? ELSE ? END,0,\
                 CASE WHEN ? THEN NULL ELSE x+20 END,CAST(x AS TEXT) FROM n"
            ).bind::<BigInt,_>((REPLY_COUNT + 1) as i64)
                .bind::<Binary,_>(source.as_ref())
                .bind::<Binary,_>(&alive_reply).bind::<Binary,_>(&deleted_reply)
                .bind::<Integer,_>(status as i32).bind::<Integer,_>(ContentType::Reply as i32)
                .bind::<Binary,_>(vec![1; 32]).bind::<Binary,_>(vec![2; 32])
                .bind::<Bool,_>(recovery).execute(sqlite)).unwrap();
            if !recovery {
                use xmtp_db::schema::refresh_state::dsl;
                db.raw_query(|sqlite| {
                    xmtp_db::diesel::update(dsl::refresh_state.filter(
                        dsl::entity_kind.eq(xmtp_db::refresh_state::EntityKind::DeliveryAllocator),
                    ))
                    .set(dsl::sequence_id.eq((REPLY_COUNT + 21) as i64))
                    .execute(sqlite)
                })
                .unwrap();
            }
            for (id, emoji) in [(7, "👍"), (8, "❤️")] {
                message(
                    source,
                    id,
                    20 + i64::from(id),
                    status,
                    TestContentGenerator::reaction_content(
                        &hex::encode(vec![3; 32]),
                        emoji,
                        ReactionAction::Added,
                    ),
                )
                .store(&db)
                .unwrap();
            }
            for (id, victim) in [(9, 2), (10, 4)] {
                message(
                    source,
                    id,
                    30,
                    status,
                    TestContentGenerator::delete_message_content(&hex::encode(vec![victim; 32])),
                )
                .store(&db)
                .unwrap();
                StoredMessageDeletion {
                    id: vec![id; 32],
                    group_id: source,
                    deleted_message_id: vec![victim; 32],
                    deleted_by_inbox_id: "sender".into(),
                    is_super_admin_deletion: false,
                    deleted_at_ns: 30,
                }
                .store(&db)
                .unwrap();
            }
            for id in 11..16 {
                message(
                    source,
                    id,
                    31,
                    status,
                    TestContentGenerator::reply_content(
                        &hex::encode(vec![3; 32]),
                        TextCodec::content_type(),
                        b"counted reply".to_vec(),
                    ),
                )
                .store(&db)
                .unwrap();
            }
            let cursor = db.current_delivery_cursor().unwrap();
            let query = MsgQueryArgs {
                sent_after_ns: Some(9),
                sent_before_ns: Some(14),
                limit: Some((REPLY_COUNT + 2 + usize::from(stitched)) as i64),
                sort_by: Some(if recovery {
                    SortBy::SentAt
                } else {
                    SortBy::SentAtDelivery
                }),
                ..Default::default()
            };
            let assert_page = |messages: &[EnrichedStoredMessage]| {
                let mut expected = Vec::new();
                if stitched {
                    expected.push(vec![5; 32]);
                }
                expected.extend([vec![3; 32], vec![4; 32]]);
                expected.extend((1..=REPLY_COUNT).map(|id| format!("{id:032}").into_bytes()));
                assert_eq!(
                    messages
                        .iter()
                        .map(|row| row.stored.id.clone())
                        .collect::<Vec<_>>(),
                    expected
                );
                let offset = usize::from(stitched);
                assert_eq!(messages[offset].decoded.num_replies, 5);
                assert_eq!(
                    messages[offset]
                        .decoded
                        .reactions
                        .iter()
                        .map(|row| row.metadata.id.clone())
                        .collect::<Vec<_>>(),
                    vec![vec![7; 32], vec![8; 32]]
                );
                assert!(matches!(
                    messages[offset + 1].decoded.content,
                    MessageBody::DeletedMessage { .. }
                ));
                for (index, row) in messages[offset + 2..].iter().enumerate() {
                    assert_eq!(row.stored.group_id, source);
                    assert_eq!(row.delivery_cursor.is_none(), recovery);
                    let MessageBody::Reply(reply) = &row.decoded.content else {
                        panic!("expected reply");
                    };
                    let parent = reply.in_reply_to.as_ref().unwrap();
                    assert_eq!(
                        parent.metadata.id,
                        vec![if index % 2 == 0 { 1 } else { 2 }; 32]
                    );
                    assert_eq!(row.parent_stored.as_ref().unwrap().group_id, source);
                    if index % 2 == 0 {
                        let MessageBody::Text(text) = &parent.content else {
                            panic!("expected live parent");
                        };
                        assert_eq!(text.content, "alive parent");
                    } else {
                        assert!(matches!(parent.content, MessageBody::DeletedMessage { .. }));
                    }
                }
            };
            let remaining = format!("{:032}", REPLY_COUNT + 1).into_bytes();
            if recovery {
                let page = recovery_page_with_stored(
                    &db,
                    &groups[0].id,
                    RecoveryQueryArgs {
                        messages: query.clone(),
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_page(&page.messages);
                assert!(page.has_more);
                assert_eq!(
                    page.first_position.unwrap().sent_at_ns,
                    if stitched { 10 } else { 11 }
                );
                let last = page.last_position.unwrap();
                assert_eq!(last.message_id, format!("{REPLY_COUNT:032}").into_bytes());
                let next = recovery_page_with_stored(
                    &db,
                    &groups[0].id,
                    RecoveryQueryArgs {
                        messages: query,
                        after: Some(last),
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(
                    next.messages
                        .iter()
                        .map(|row| row.stored.id.clone())
                        .collect::<Vec<_>>(),
                    vec![remaining]
                );
                assert!(!next.has_more);
            } else {
                let page = history_page_with_stored(&db, &groups[0].id, &query).unwrap();
                assert_page(&page.messages);
                assert!(page.has_more);
                assert_eq!(
                    page.first_position.unwrap().sent_at_ns,
                    if stitched { 10 } else { 11 }
                );
                let last = page.last_position.unwrap();
                assert_eq!(last.cursor.delivery_sequence, (REPLY_COUNT + 20) as u64);
                let mut next_query = query;
                next_query.history_after = Some(last);
                let next = history_page_with_stored(&db, &groups[0].id, &next_query).unwrap();
                assert_eq!(
                    next.messages
                        .iter()
                        .map(|row| row.stored.id.clone())
                        .collect::<Vec<_>>(),
                    vec![remaining]
                );
                assert!(!next.has_more);
            }
            assert_eq!(db.current_delivery_cursor().unwrap(), cursor);
        }
    }

    #[xmtp_common::test]
    async fn history_page_large_relation_lists() {
        large_relation_lists(false).await;
    }

    #[xmtp_common::test]
    async fn recovery_page_large_relation_lists() {
        large_relation_lists(true).await;
    }
}
