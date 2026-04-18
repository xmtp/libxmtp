use crate::groups::GroupError;
use crate::groups::error::EditMessageError;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_db::group_message::{ContentType, GroupMessageKind, MsgQueryArgs};
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
