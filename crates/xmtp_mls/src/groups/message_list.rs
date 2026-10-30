use crate::context::XmtpSharedContext;
use crate::groups::MlsGroup;
use crate::messages::decoded_message::DecodedMessage;
use crate::messages::enrichment::{
    EnrichMessageError, EnrichedStoredMessage, enrich_messages, enrich_messages_with_stored,
};
use xmtp_db::DbQuery;
use xmtp_db::group_message::{ContentType as DbContentType, MsgQueryArgs};
use xmtp_db::prelude::QueryGroupMessage;

mod page;
pub use page::{EnrichedHistoryPage, EnrichedRecoveryPage};

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    #[xmtp_common::mls_span]
    pub fn find_messages_v2(
        &self,
        query: &MsgQueryArgs,
    ) -> Result<Vec<DecodedMessage>, EnrichMessageError> {
        let conn = self.context.db();
        self.find_messages_v2_with_conn(query, conn)
    }

    #[xmtp_common::mls_span]
    pub fn find_messages_v2_with_stored(
        &self,
        query: &MsgQueryArgs,
    ) -> Result<Vec<EnrichedStoredMessage>, EnrichMessageError> {
        let conn = self.context.db();
        use xmtp_db::delivery::QueryDelivery;
        let rows = conn
            .app_visible_message_rows(
                &self.group_id,
                &filter_out_hidden_message_types_from_query(query),
            )
            .map_err(EnrichMessageError::Storage)?;
        let cursors: std::collections::HashMap<_, _> = rows
            .iter()
            .map(|row| (row.stored.id.clone(), row.cursor))
            .collect();
        let mut enriched = enrich_messages_with_stored(
            conn,
            &self.group_id,
            rows.into_iter().map(|row| row.stored).collect(),
        )?;
        for message in &mut enriched {
            message.delivery_cursor = cursors.get(&message.stored.id).copied().flatten();
        }
        Ok(enriched)
    }

    #[xmtp_common::mls_span]
    pub fn find_messages_v2_with_conn<C>(
        &self,
        query: &MsgQueryArgs,
        conn: C,
    ) -> Result<Vec<DecodedMessage>, EnrichMessageError>
    where
        C: QueryGroupMessage + DbQuery,
    {
        let initial_messages = conn.get_group_messages(
            &self.group_id,
            &filter_out_hidden_message_types_from_query(query),
        )?;

        enrich_messages(conn, &self.group_id, initial_messages)
    }
}

fn filter_out_hidden_message_types_from_query(query: &MsgQueryArgs) -> MsgQueryArgs {
    let mut new_query = query.clone();
    let hidden_message_types = vec![
        DbContentType::Reaction,
        DbContentType::ReadReceipt,
        DbContentType::DeleteMessage,
    ];

    let excluded_content_types = match &query.exclude_content_types {
        Some(excluded) => {
            let mut content_types = excluded.clone();
            content_types.extend(
                hidden_message_types
                    .into_iter()
                    .filter(|t| !excluded.contains(t)),
            );
            content_types
        }
        None => hidden_message_types,
    };
    new_query.exclude_content_types = Some(excluded_content_types);
    new_query
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::ClientBuilder;
    use crate::groups::QueryableContentFields;
    use crate::messages::decoded_message::{ContentDecodeFailureKind, MessageBody};
    use hex::ToHexExt;
    use xmtp_common::time::now_ns;
    use xmtp_content_types::ContentCodec;
    use xmtp_content_types::reaction::ReactionCodec;
    use xmtp_content_types::test_utils::TestContentGenerator;
    use xmtp_content_types::text::TextCodec;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::Store;
    use xmtp_db::group_message::{
        ContentType as DbContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage,
    };
    use xmtp_proto::types::GroupId;
    use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionAction;
    use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

    async fn setup_test_group() -> (MlsGroup<impl XmtpSharedContext>, impl XmtpSharedContext) {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client.create_group(None, Default::default()).unwrap();

        (group, client.context.clone())
    }

    fn create_test_message(
        group_id: &GroupId,
        message_id: Vec<u8>,
        encoded_content: EncodedContent,
        sent_at_ns: i64,
    ) -> StoredGroupMessage {
        let content_bytes = xmtp_content_types::encoded_content_to_bytes(encoded_content.clone());
        let queryable_fields =
            QueryableContentFields::try_from(encoded_content).unwrap_or_default();

        StoredGroupMessage {
            id: message_id,
            group_id: *group_id,
            decrypted_message_bytes: content_bytes,
            sent_at_ns,
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id: "sender".into(),
            delivery_status: DeliveryStatus::Published,
            content_type: queryable_fields.content_type,
            version_major: queryable_fields.version_major,
            version_minor: queryable_fields.version_minor,
            authority_id: queryable_fields.authority_id,
            reference_id: queryable_fields.reference_id,
            sequence_id: 0,
            envelope_hash: None,
            expiry_ns: None,
            expiry_unknown: false,
            expire_at_ns: None,
            inserted_at_ns: 0,
            should_push: true,
            idempotency_key: sent_at_ns.to_string(),
        }
    }

    // For tests that need malformed content
    fn create_test_message_raw(
        group_id: &GroupId,
        message_id: Vec<u8>,
        content: Vec<u8>,
        sent_at_ns: i64,
        content_type_id: Option<ContentTypeId>,
        reference_id: Option<Vec<u8>>,
    ) -> StoredGroupMessage {
        let queryable_fields = content_type_id
            .as_ref()
            .map(|ct| {
                (
                    DbContentType::from_identifier(&ct.authority_id, &ct.type_id, ct.version_major),
                    ct.version_major as i32,
                    ct.version_minor as i32,
                    ct.authority_id.clone(),
                )
            })
            .unwrap_or((DbContentType::Text, 1, 0, "xmtp.org".to_string()));

        StoredGroupMessage {
            id: message_id,
            group_id: *group_id,
            decrypted_message_bytes: content,
            sent_at_ns,
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id: "sender".into(),
            delivery_status: DeliveryStatus::Published,
            content_type: queryable_fields.0,
            version_major: queryable_fields.1,
            version_minor: queryable_fields.2,
            authority_id: queryable_fields.3,
            reference_id,
            sequence_id: 0,
            envelope_hash: None,
            expiry_ns: None,
            expiry_unknown: false,
            expire_at_ns: None,
            inserted_at_ns: 0,
            should_push: true,
            idempotency_key: sent_at_ns.to_string(),
        }
    }

    /// Stores message `n` with id `[n; 32]`, sent `n` microseconds from now.
    fn create_and_store_message<S>(conn: &S, group_id: &GroupId, n: u8, content: EncodedContent)
    where
        StoredGroupMessage: Store<S>,
    {
        let sent_at_ns = now_ns() + i64::from(n) * 1000;
        create_test_message(group_id, vec![n; 32], content, sent_at_ns)
            .store(conn)
            .unwrap();
    }

    /// The hex reference to message `n`.
    fn ref_to(n: u8) -> String {
        hex::encode([n; 32])
    }

    /// The `n` of each message stored with id `[n; 32]`.
    fn ids(messages: &[DecodedMessage]) -> Vec<u8> {
        messages
            .iter()
            .map(|message| message.metadata.id[0])
            .collect()
    }

    fn emojis(message: &DecodedMessage) -> Vec<&str> {
        message
            .reactions
            .iter()
            .filter_map(|reaction| match &reaction.content {
                MessageBody::Reaction(reaction) => Some(reaction.content.as_str()),
                _ => None,
            })
            .collect()
    }

    // ===== Tests =====

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_exclude_content_types_with_custom_exclusions() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();
        create_and_store_message(
            &conn,
            &group.group_id,
            1,
            TestContentGenerator::text_content("Hello World"),
        );

        // Store a locally generated GroupUpdated transcript.
        let mut transcript = create_test_message(
            &group.group_id,
            vec![2; 32],
            TestContentGenerator::group_updated_content(vec!["inbox1".to_string()]),
            now_ns() + 2000,
        );
        transcript.kind = GroupMessageKind::MembershipChange;
        transcript.store(&conn)?;

        let messages = group.find_messages_v2(&MsgQueryArgs::default())?;
        assert_eq!(ids(&messages), [1, 2]);

        // Excluding Text returns only the GroupUpdated message
        let query = MsgQueryArgs {
            exclude_content_types: Some(vec![DbContentType::Text]),
            ..Default::default()
        };
        let messages = group.find_messages_v2(&query)?;
        assert_eq!(ids(&messages), [2]);
        assert!(
            matches!(messages[0].content, MessageBody::GroupUpdated(_)),
            "{:?}",
            messages[0].content
        );
    }

    /// Reactions attach to their target and are hidden from the list, as are
    /// read receipts and deletions. A malformed reaction row is not attached.
    #[xmtp_common::test(unwrap_try = true)]
    async fn reactions_attach_to_their_target_and_hidden_types_are_filtered() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();
        let put = |n, content| create_and_store_message(&conn, &group.group_id, n, content);

        put(1, TestContentGenerator::text_content("Hello World"));
        let reaction = |emoji| {
            TestContentGenerator::reaction_content(&ref_to(1), emoji, ReactionAction::Added)
        };
        put(2, reaction("👍"));
        put(3, reaction("❤️"));
        create_test_message_raw(
            &group.group_id,
            vec![4; 32],
            b"invalid reaction bytes".to_vec(),
            now_ns() + 4000,
            Some(ReactionCodec::content_type()),
            Some(vec![1; 32]),
        )
        .store(&conn)?;
        put(5, TestContentGenerator::read_receipt_content());
        put(6, TestContentGenerator::delete_message_content(&ref_to(1)));
        put(7, TestContentGenerator::text_content("Second message"));

        let messages = group.find_messages_v2(&MsgQueryArgs::default())?;
        assert_eq!(ids(&messages), [1, 7], "hidden types are filtered");
        assert!(
            matches!(&messages[0].content, MessageBody::Text(text) if text.content == "Hello World")
        );
        assert!(
            matches!(&messages[1].content, MessageBody::Text(text) if text.content == "Second message")
        );
        assert_eq!(emojis(&messages[0]), ["👍", "❤️"]);
        assert_eq!(messages[0].reactions.len(), 2, "malformed reaction");
        assert!(messages[1].reactions.is_empty());
    }

    /// Each message carries its 50 latest reaction messages, and a count of
    /// all of its unexpired ones, on every read path. A parent with fewer
    /// reactions keeps all of them.
    // verifies: PROC-053, META-051
    #[xmtp_common::test(unwrap_try = true)]
    async fn history_reaction_summary_is_bounded_per_message() {
        const REACTION_COUNT: i64 = 256;
        const SUMMARY: usize = 50;

        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client.create_group(None, Default::default())?;
        let conn = client.context.db();
        let base = now_ns();
        let busy_parent = vec![0xa5; 32];
        let quiet_parent = vec![0xb6; 32];
        for parent in [&busy_parent, &quiet_parent] {
            create_test_message(
                &group.group_id,
                parent.clone(),
                TestContentGenerator::text_content("target"),
                base,
            )
            .store(&conn)?;
        }
        let reaction = |parent: &[u8], id: Vec<u8>, offset: i64| {
            create_test_message(
                &group.group_id,
                id,
                TestContentGenerator::reaction_content(
                    &parent.encode_hex(),
                    "👍",
                    ReactionAction::Added,
                ),
                base + offset,
            )
        };
        // Each row is a distinct, accepted reaction from a participant.
        for offset in 1..=REACTION_COUNT {
            reaction(&busy_parent, offset.to_be_bytes().to_vec(), offset).store(&conn)?;
        }
        // The newest reaction has expired, so it neither shows nor counts.
        let mut expired = reaction(&busy_parent, vec![0xee; 8], REACTION_COUNT + 1);
        expired.expire_at_ns = Some(now_ns() - 1);
        expired.store(&conn)?;
        for offset in 1..=3 {
            reaction(&quiet_parent, vec![0xb6, offset as u8], offset).store(&conn)?;
        }

        let assert_summary = |message: &DecodedMessage| {
            let offsets: Vec<_> = message
                .reactions
                .iter()
                .map(|reaction| reaction.metadata.sent_at_ns - base)
                .collect();
            if message.metadata.id == busy_parent {
                let latest: Vec<_> =
                    (REACTION_COUNT - SUMMARY as i64 + 1..=REACTION_COUNT).collect();
                assert_eq!(offsets, latest);
                assert_eq!(message.num_reaction_messages, REACTION_COUNT as usize);
            } else {
                assert_eq!(offsets, [1, 2, 3]);
                assert_eq!(message.num_reaction_messages, 3);
            }
        };

        let history = group.find_messages_v2(&MsgQueryArgs::default())?;
        assert_eq!(history.len(), 2);
        history.iter().for_each(assert_summary);
        for parent in [&busy_parent, &quiet_parent] {
            assert_summary(&client.message_v2(parent.clone())?);
        }
        let sdk_history = group.find_messages_v2_with_stored(&MsgQueryArgs::default())?;
        assert_eq!(sdk_history.len(), 2);
        sdk_history
            .iter()
            .for_each(|message| assert_summary(&message.decoded));
    }

    /// A reply carries its parent one level deep. A reference to a missing
    /// message, or one that is not a message id, leaves the reply without one.
    #[xmtp_common::test(unwrap_try = true)]
    async fn replies_resolve_one_level_of_parent() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();
        let put = |n, content| create_and_store_message(&conn, &group.group_id, n, content);
        let reply_to = |reference: &str| {
            let content = b"reply".to_vec();
            TestContentGenerator::reply_content(reference, TextCodec::content_type(), content)
        };

        put(1, TestContentGenerator::text_content("Original"));
        put(2, reply_to(&ref_to(1)));
        put(3, reply_to(&ref_to(2)));
        put(4, reply_to(&ref_to(99)));
        put(5, reply_to("not-valid-hex!@#"));

        let messages = group.find_messages_v2(&MsgQueryArgs::default())?;
        assert_eq!(ids(&messages), [1, 2, 3, 4, 5]);
        let reply = |n: usize| match &messages[n - 1].content {
            MessageBody::Reply(reply) => reply,
            other => panic!("message {n}: expected reply, got {other:?}"),
        };
        for (n, reference, parent) in [
            (2, ref_to(1), Some(1)),
            (3, ref_to(2), Some(2)),
            (4, ref_to(99), None),
            (5, "not-valid-hex!@#".into(), None),
        ] {
            let in_reply_to = reply(n).in_reply_to.as_ref();
            assert_eq!(reply(n).reference_id, reference, "reply {n}");
            assert_eq!(in_reply_to.map(|p| p.metadata.id[0]), parent, "reply {n}");
        }
        let parent = |n| reply(n).in_reply_to.as_deref().expect("parent");
        assert!(
            matches!(&parent(2).content, MessageBody::Text(text) if text.content == "Original")
        );
        assert!(
            matches!(&parent(3).content, MessageBody::Reply(inner) if inner.in_reply_to.is_none()),
            "parents are not resolved recursively"
        );
    }

    /// In a stitched DM, a deletion applies to its target, and to a reply's
    /// parent, only when it is stored in the target's group.
    // verifies: PROC-037
    #[xmtp_common::test(unwrap_try = true)]
    async fn stitched_deletions_apply_in_their_own_group() {
        use xmtp_common::Generate;
        use xmtp_db::ConnectionExt;
        use xmtp_db::diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
        use xmtp_db::group::{ConversationType, QueryGroup};
        use xmtp_db::message_deletion::StoredMessageDeletion;
        use xmtp_db::schema::groups::dsl;

        let (group, context) = setup_test_group().await;
        let conn = context.db();
        let home = group.group_id;
        conn.raw_query(|sqlite| {
            xmtp_db::diesel::update(dsl::groups.find(home))
                .set((
                    dsl::conversation_type.eq(ConversationType::Dm),
                    dsl::dm_id.eq("dm:sender:peer"),
                ))
                .execute(sqlite)
        })?;
        let mut sibling = conn.find_group(&home)??;
        sibling.id = GroupId::generate();
        sibling.store(&conn)?;
        let sibling = sibling.id;
        let put = |group_id, n, content| create_and_store_message(&conn, &group_id, n, content);
        let reply_to = |n| {
            let content = b"reply".to_vec();
            TestContentGenerator::reply_content(&ref_to(n), TextCodec::content_type(), content)
        };
        put(sibling, 1, TestContentGenerator::text_content("deleted"));
        put(sibling, 2, TestContentGenerator::text_content("kept"));
        put(home, 3, reply_to(1));
        put(home, 4, reply_to(2));
        // Message 1 is deleted in its own group, message 2 from the other one.
        for (group_id, n, target) in [(sibling, 5, 1), (home, 6, 2)] {
            let deletion = TestContentGenerator::delete_message_content(&ref_to(target));
            put(group_id, n, deletion);
            StoredMessageDeletion {
                id: vec![n; 32],
                group_id,
                deleted_message_id: vec![target; 32],
                deleted_by_inbox_id: "sender".into(),
                is_super_admin_deletion: false,
                deleted_at_ns: now_ns(),
            }
            .store(&conn)?;
        }

        let messages = group.find_messages_v2_with_stored(&MsgQueryArgs::default())?;
        let messages: Vec<_> = messages.into_iter().map(|m| m.decoded).collect();
        assert_eq!(ids(&messages), [1, 2, 3, 4]);
        let parent = |n: usize| match &messages[n - 1].content {
            MessageBody::Reply(reply) => reply.in_reply_to.as_deref().expect("parent"),
            other => panic!("message {n}: expected reply, got {other:?}"),
        };
        // The text shown, or None for a deletion placeholder.
        let text = |message: &DecodedMessage| match &message.content {
            MessageBody::Text(text) => Some(text.content.clone()),
            MessageBody::DeletedMessage { .. } => None,
            other => panic!("expected text or a deletion, got {other:?}"),
        };
        assert_eq!(text(&messages[0]), None, "target 1");
        assert_eq!(text(parent(3)), None, "parent of 3");
        assert_eq!(text(&messages[1]), Some("kept".into()), "target 2");
        assert_eq!(text(parent(4)), Some("kept".into()), "parent of 4");
    }

    // verifies: CTYPE-008, CTYPE-009
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_find_messages_undecodable_messages() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();
        let put = |n, content| create_and_store_message(&conn, &group.group_id, n, content);

        put(1, TestContentGenerator::text_content("Valid message"));
        // An old row: bytes that are not an EncodedContent, cached as Text
        // the way a client stored it before classification was checked.
        let malformed = vec![0xff, 0x00, 0x80];
        create_test_message_raw(
            &group.group_id,
            vec![2; 32],
            malformed.clone(),
            now_ns() + 2000,
            None,
            None,
        )
        .store(&conn)?;
        // A complete typed envelope with no core codec
        put(3, TestContentGenerator::invalid_content());

        let messages = group.find_messages_v2(&MsgQueryArgs::default())?;
        assert_eq!(ids(&messages), [1, 2, 3]);
        assert!(
            matches!(&messages[0].content, MessageBody::Text(text) if text.content == "Valid message")
        );

        assert_eq!(messages[1].metadata.content_type, None);
        let MessageBody::Undecodable(undecodable) = &messages[1].content else {
            panic!(
                "expected undecodable content, got {:?}",
                messages[1].content
            );
        };
        assert_eq!(undecodable.raw_bytes, malformed);
        assert_eq!(
            undecodable.failure.kind,
            ContentDecodeFailureKind::MalformedEnvelope
        );

        let MessageBody::Custom(content) = &messages[2].content else {
            panic!("expected custom content for unknown type message");
        };
        assert_eq!(
            content.encoded.fallback,
            Some("Invalid message".to_string())
        );
        assert_eq!(
            messages[2].fallback_text,
            Some("Invalid message".to_string())
        );
    }

    /// History, lookup by id, and a reply's parent all keep a malformed row
    /// with its id and exact bytes, and later rows stay visible.
    // verifies: CTYPE-008
    #[xmtp_common::test(unwrap_try = true)]
    async fn malformed_rows_are_kept_on_history_lookup_and_parent_paths() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client.create_group(None, Default::default())?;
        let conn = client.context.db();
        let put = |n, content| create_and_store_message(&conn, &group.group_id, n, content);

        let malformed = vec![0xff, 0x00, 0x80];
        let malformed_id = vec![7; 32];
        create_test_message_raw(
            &group.group_id,
            malformed_id.clone(),
            malformed.clone(),
            now_ns(),
            None,
            None,
        )
        .store(&conn)?;
        let reply = TestContentGenerator::reply_content(
            &ref_to(7),
            TextCodec::content_type(),
            b"replying to bytes".to_vec(),
        );
        put(8, reply);
        put(9, TestContentGenerator::text_content("later"));

        let messages = group.find_messages_v2(&MsgQueryArgs::default())?;
        assert_eq!(ids(&messages), [7, 8, 9]);
        let is_kept = |message: &DecodedMessage| {
            matches!(&message.content, MessageBody::Undecodable(undecodable)
                if undecodable.raw_bytes == malformed
                    && undecodable.failure.kind == ContentDecodeFailureKind::MalformedEnvelope)
        };
        assert!(is_kept(&messages[0]), "history: {:?}", messages[0].content);
        assert!(matches!(&messages[2].content, MessageBody::Text(text) if text.content == "later"));

        let MessageBody::Reply(reply) = &messages[1].content else {
            panic!("expected reply, got {:?}", messages[1].content);
        };
        let parent = reply.in_reply_to.as_ref().expect("reply parent");
        assert_eq!(parent.metadata.id, malformed_id);
        assert!(is_kept(parent), "parent: {:?}", parent.content);

        let looked_up = client.message_v2(malformed_id.clone())?;
        assert_eq!(looked_up.metadata.id, malformed_id);
        assert!(is_kept(&looked_up), "lookup: {:?}", looked_up.content);
    }

    /// A read with no limit enriches every row, past SQLite's bound-value limit.
    #[xmtp_common::test(unwrap_try = true)]
    async fn unlimited_history_read_spans_the_bind_limit() {
        use xmtp_db::ConnectionExt;
        use xmtp_db::diesel::RunQueryDsl;
        use xmtp_db::diesel::sql_types::{BigInt, Binary, Integer};
        const ROWS: i64 = 33_000;

        let (group, context) = setup_test_group().await;
        let conn = context.db();
        let text =
            xmtp_content_types::encoded_content_to_bytes(TestContentGenerator::text_content("row"));
        conn.raw_query(|sqlite| {
            xmtp_db::diesel::sql_query(
                "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<?) \
                 INSERT INTO group_messages (id,group_id,decrypted_message_bytes,sent_at_ns,kind,\
                 sender_installation_id,sender_inbox_id,delivery_status,content_type,version_major,\
                 version_minor,authority_id,sequence_id,idempotency_key) \
                 SELECT CAST(printf('%032d',x) AS BLOB),?,?,x,?,x'01','sender',?,?,1,0,\
                 'xmtp.org',0,CAST(x AS TEXT) FROM n",
            )
            .bind::<BigInt, _>(ROWS)
            .bind::<Binary, _>(group.group_id.as_ref())
            .bind::<Binary, _>(&text)
            .bind::<Integer, _>(GroupMessageKind::Application as i32)
            .bind::<Integer, _>(DeliveryStatus::Published as i32)
            .bind::<Integer, _>(DbContentType::Text as i32)
            .execute(sqlite)
        })?;
        let newest = format!("{ROWS:032}").into_bytes();
        create_test_message(
            &group.group_id,
            vec![0xee; 32],
            TestContentGenerator::reaction_content(
                &newest.encode_hex(),
                "👍",
                ReactionAction::Added,
            ),
            ROWS + 1,
        )
        .store(&conn)?;

        let ascending: Vec<i64> = (1..=ROWS).collect();
        let stored = group.find_messages_v2_with_stored(&MsgQueryArgs::default())?;
        let sent_at: Vec<_> = stored.iter().map(|m| m.stored.sent_at_ns).collect();
        assert_eq!(sent_at, ascending);
        assert_eq!(emojis(&stored.last()?.decoded), ["👍"]);
        let decoded = group.find_messages_v2(&MsgQueryArgs::default())?;
        let sent_at: Vec<_> = decoded.iter().map(|m| m.metadata.sent_at_ns).collect();
        assert_eq!(sent_at, ascending);
        assert_eq!(emojis(decoded.last()?), ["👍"]);
    }
}
