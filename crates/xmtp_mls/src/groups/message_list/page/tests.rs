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
}
