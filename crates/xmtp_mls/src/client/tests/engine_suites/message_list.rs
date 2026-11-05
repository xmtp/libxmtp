//! The leaves of `groups/message_list.rs` that need the client.

use crate::builder::ClientBuilder;
use crate::groups::test_support::message_rows::{
    create_and_store_message, create_test_message, create_test_message_raw, ids, ref_to,
};
use crate::messages::decoded_message::DecodedMessage;
use crate::messages::decoded_message::{ContentDecodeFailureKind, MessageBody};
use crate::prelude::*;
use hex::ToHexExt;
use xmtp_common::time::now_ns;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::test_utils::TestContentGenerator;
use xmtp_content_types::text::TextCodec;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::Store;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionAction;

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
            let latest: Vec<_> = (REACTION_COUNT - SUMMARY as i64 + 1..=REACTION_COUNT).collect();
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

/// A reaction over the reaction limit stores no reference, so it is left out
/// of its parent's summary and its count, on history and lookup.
// verifies: CTYPE-034, PROC-053
#[xmtp_common::test(unwrap_try = true)]
async fn reaction_summary_drops_oversized_reactions() {
    use prost::Message;
    use xmtp_content_types::{compression, reaction::ReactionCodec};
    use xmtp_proto::xmtp::mls::message_contents::{
        Compression, EncodedContent, content_types::ReactionV2,
    };

    const OVERSIZED: i64 = 5;

    let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let group = client.create_group(None, Default::default())?;
    let conn = client.context.db();
    let base = now_ns();
    let parent = vec![0xc7; 32];
    create_test_message(
        &group.group_id,
        parent.clone(),
        TestContentGenerator::text_content("target"),
        base,
    )
    .store(&conn)?;
    create_test_message(
        &group.group_id,
        vec![0xc7, 0],
        TestContentGenerator::reaction_content(&parent.encode_hex(), "👍", ReactionAction::Added),
        base + 1,
    )
    .store(&conn)?;
    // Each expands to 1 MiB of reaction content from a small compressed row.
    let oversized = EncodedContent {
        r#type: Some(ReactionCodec::content_type()),
        content: ReactionV2 {
            reference: parent.encode_hex(),
            action: ReactionAction::Added.into(),
            content: "a".repeat(1 << 20),
            ..Default::default()
        }
        .encode_to_vec(),
        ..Default::default()
    };
    let oversized = compression::compress(oversized, Compression::Deflate)?;
    for offset in 1..=OVERSIZED {
        create_test_message(
            &group.group_id,
            vec![0xc7, offset as u8],
            oversized.clone(),
            base + 1 + offset,
        )
        .store(&conn)?;
    }

    let assert_summary = |message: &DecodedMessage| {
        assert_eq!(message.metadata.id, parent);
        let contents: Vec<String> = message
            .reactions
            .iter()
            .map(|reaction| match &reaction.content {
                // Short prefixes keep a failure readable.
                MessageBody::Reaction(reaction) => reaction.content.chars().take(8).collect(),
                _ => panic!("summary holds a non-reaction"),
            })
            .collect();
        assert_eq!(contents, ["👍"]);
        assert_eq!(message.num_reaction_messages, 1);
    };
    let history = group.find_messages_v2(&MsgQueryArgs::default())?;
    assert_eq!(history.len(), 1);
    assert_summary(&history[0]);
    assert_summary(&client.message_v2(parent.clone())?);
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
