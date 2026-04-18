use crate::groups::GroupError;
use crate::groups::error::EditMessageError;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_db::group_message::{ContentType, GroupMessageKind, MsgQueryArgs, QueryGroupMessage};
use xmtp_db::message_edit::QueryMessageEdit;

#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_message_by_sender() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    let original = TextCodec::encode("Hello original".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(original);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let edited = TextCodec::encode("Hello edited".to_string())?;
    let edit_id = alix_group.edit_message(message_id.clone(), edited)?;

    assert!(!edit_id.is_empty());

    let conn = alix.context.db();
    assert!(conn.is_message_edited(&message_id)?);

    let edit = conn.get_latest_edit_by_message_id(&message_id)?.unwrap();
    assert_eq!(edit.edited_by_inbox_id, alix.inbox_id());
    assert_eq!(edit.edited_message_id, message_id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_message_authorization_failure() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    let text = TextCodec::encode("Alix's message".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    bo_group.sync().await?;

    let edited = TextCodec::encode("Bo tries to edit".to_string())?;
    let result = bo_group.edit_message(message_id, edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::NotAuthorized))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_nonexistent_message() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let edited = TextCodec::encode("nope".to_string())?;
    let result = alix_group.edit_message(vec![1, 2, 3, 4, 5], edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::MessageNotFound(_)))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_cannot_edit_others_message() {
    // Alix is group creator / super admin; Bo is a regular member.
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    // Bo sends a message.
    let text = TextCodec::encode("Bo's message".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = bo_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    alix_group.sync().await?;

    // Admin Alix attempts to edit Bo's message — must fail per XIP-77.
    let edited = TextCodec::encode("Alix tries admin edit".to_string())?;
    let result = alix_group.edit_message(message_id, edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::NotAuthorized))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_edit_transcript_messages() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    let messages = alix_group.find_messages(&MsgQueryArgs {
        kind: Some(GroupMessageKind::MembershipChange),
        ..Default::default()
    })?;
    assert!(!messages.is_empty());

    let edited = TextCodec::encode("nope".to_string())?;
    let result = alix_group.edit_message(messages[0].id.clone(), edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::NonEditableMessage))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_edit_edit_message() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("original".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let edited = TextCodec::encode("v2".to_string())?;
    let edit_message_id = alix_group.edit_message(message_id, edited)?;

    let edited_again = TextCodec::encode("v3".to_string())?;
    let result = alix_group.edit_message(edit_message_id, edited_again);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::NonEditableMessage))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_edit_deleted_message() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("soon deleted".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;

    let edited = TextCodec::encode("too late".to_string())?;
    let result = alix_group.edit_message(message_id, edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::MessageDeleted))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_edit_message_from_different_group() {
    tester!(alix);
    tester!(bo);
    let group1 = alix.create_group(None, None)?;
    let group2 = alix.create_group(None, None)?;
    group1.add_members(&[bo.inbox_id()]).await?;
    group2.add_members(&[bo.inbox_id()]).await?;

    let text = TextCodec::encode("in group1".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = group1
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let edited = TextCodec::encode("from group2".to_string())?;
    let result = group2.edit_message(message_id, edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::NotAuthorized))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_enrichment_shows_edited_content() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("before edit".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let edited = TextCodec::encode("after edit".to_string())?;
    alix_group.edit_message(message_id.clone(), edited)?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    let enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let msg = enriched.iter().find(|m| m.metadata.id == message_id).unwrap();

    // Content should be the edited version
    match &msg.content {
        crate::messages::decoded_message::MessageBody::Text(t) => {
            assert_eq!(t.content, "after edit");
        }
        other => panic!("Expected Text body with edited content, got {:?}", other),
    }

    // edited field should be set so consumers can show "(edited)"
    assert_eq!(
        msg.edited,
        Some(crate::messages::decoded_message::EditedBy::Sender)
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_multiple_edits_latest_wins() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("v1".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let v2 = TextCodec::encode("v2".to_string())?;
    alix_group.edit_message(message_id.clone(), v2)?;

    let v3 = TextCodec::encode("v3".to_string())?;
    alix_group.edit_message(message_id.clone(), v3)?;

    let conn = alix.context.db();
    let latest = conn.get_latest_edit_by_message_id(&message_id)?.unwrap();
    let content = xmtp_content_types::bytes_to_encoded_content(latest.edited_content_bytes);
    let decoded_text = TextCodec::decode(content)?;
    assert_eq!(decoded_text, "v3");
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_enrichment_preserves_reactions_after_edit() {
    use xmtp_content_types::reaction::ReactionCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2;

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    let text = TextCodec::encode("react to me".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    let reaction = ReactionV2 {
        reference: hex::encode(&message_id),
        reference_inbox_id: bo.inbox_id().to_string(),
        action: 1, // ReactionAction::Added
        content: "👍".to_string(),
        schema: 1, // ReactionSchema::Unicode
    };
    let reaction_bytes =
        xmtp_content_types::encoded_content_to_bytes(ReactionCodec::encode(reaction)?);
    bo_group
        .send_message(&reaction_bytes, SendMessageOpts::default())
        .await?;
    bo_group.publish_messages().await?;
    alix_group.sync().await?;

    let edited = TextCodec::encode("edited with reaction".to_string())?;
    alix_group.edit_message(message_id.clone(), edited)?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    let enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let msg = enriched.iter().find(|m| m.metadata.id == message_id).unwrap();

    assert!(
        !msg.reactions.is_empty(),
        "Reactions must be preserved after edit"
    );
}
