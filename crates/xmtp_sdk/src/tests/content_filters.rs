use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn reaction_message_keeps_its_target_on_single_read_and_reader() {
    use crate::{Reaction, ReactionAction, ReactionSchema};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent = group.send_text("parent".into(), None).await?;
    let parent_sender = client.inbox_id();
    let reader = group.message_reader().await?;
    let reaction_id = client
        .conversations()
        .react_to_message(
            parent.clone(),
            Reaction {
                content: "👍".into(),
                action: ReactionAction::Added,
                schema: ReactionSchema::Unicode,
            },
            None,
        )
        .await?;
    let by_id = client
        .conversations()
        .get_message_by_id(reaction_id.clone())
        .await?
        .expect("reaction by ID");
    let received = xmtp_common::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if let Some(message) = reader.next().await?
                && message.0.id == reaction_id
            {
                break Ok::<_, crate::XmtpError>(message);
            }
        }
    })
    .await??;
    for (path, message) in [("by ID", by_id), ("reader", received)] {
        let MessageContent::Reaction {
            reference,
            reference_inbox_id,
            reaction,
        } = message.0.content
        else {
            panic!("{path} did not return reaction content");
        };
        assert_eq!(reference, parent, "{path} lost the target message ID");
        assert_eq!(
            reference_inbox_id,
            Some(parent_sender.clone()),
            "{path} lost the target sender"
        );
        assert_eq!(reaction.content, "👍");
    }
    reader.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
fn query_filters_match_stored_catalogue_types() {
    use xmtp_content_types::{
        ContentCodec, actions::ActionsCodec, attachment::AttachmentCodec,
        delete_message::DeleteMessageCodec, group_updated::GroupUpdatedCodec, intent::IntentCodec,
        leave_request::LeaveRequestCodec, markdown::MarkdownCodec,
        membership_change::GroupMembershipChangeCodec,
        multi_remote_attachment::MultiRemoteAttachmentCodec, reaction::ReactionCodec,
        read_receipt::ReadReceiptCodec, remote_attachment::RemoteAttachmentCodec,
        reply::ReplyCodec, text::TextCodec, transaction_reference::TransactionReferenceCodec,
        wallet_send_calls::WalletSendCallsCodec,
    };
    use xmtp_db::group_message::ContentType;

    macro_rules! check_codec {
        ($codec:ty) => {{
            let kind = <$codec>::content_type();
            let expected =
                ContentType::from_identifier(&kind.authority_id, &kind.type_id, kind.version_major);
            let actual = crate::conversation::query_content_types(vec![crate::ContentTypeId {
                authority_id: kind.authority_id,
                type_id: kind.type_id,
                version_major: kind.version_major,
                version_minor: kind.version_minor,
            }])?;
            assert_eq!(actual, vec![expected], stringify!($codec));
        }};
    }
    check_codec!(TextCodec);
    check_codec!(MarkdownCodec);
    check_codec!(GroupMembershipChangeCodec);
    check_codec!(GroupUpdatedCodec);
    check_codec!(ReactionCodec);
    check_codec!(ReadReceiptCodec);
    check_codec!(ReplyCodec);
    check_codec!(AttachmentCodec);
    check_codec!(RemoteAttachmentCodec);
    check_codec!(MultiRemoteAttachmentCodec);
    check_codec!(TransactionReferenceCodec);
    check_codec!(WalletSendCallsCodec);
    check_codec!(LeaveRequestCodec);
    check_codec!(ActionsCodec);
    check_codec!(IntentCodec);
    check_codec!(DeleteMessageCodec);

    let wrong_major = crate::ContentTypeId {
        authority_id: "xmtp.org".into(),
        type_id: "text".into(),
        version_major: 99,
        version_minor: 0,
    };
    assert!(matches!(
        crate::conversation::query_content_types(vec![wrong_major]),
        Err(XmtpError::InvalidArgument(_))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn group_updated_message_filter_finds_stored_row() {
    use crate::ListMessagesOptions;
    use xmtp_content_types::{ContentCodec, group_updated::GroupUpdatedCodec};
    use xmtp_proto::xmtp::mls::message_contents::GroupUpdated;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let content = GroupUpdatedCodec::encode(GroupUpdated {
        initiated_by_inbox_id: client.inbox_id().into_checked()?,
        ..Default::default()
    })?;
    let kind = content.r#type.clone().expect("typed content");
    let id = group.send(content.into(), None).await?;
    let messages = group
        .messages(Some(ListMessagesOptions {
            content_types: Some(vec![crate::ContentTypeId {
                authority_id: kind.authority_id,
                type_id: kind.type_id,
                version_major: kind.version_major,
                version_minor: kind.version_minor,
            }]),
            ..Default::default()
        }))
        .await?;
    assert!(messages.iter().any(|message| message.0.id == id));
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn custom_content_type_filter_rejects_unknown_storage_type() {
    use crate::{ContentTypeId, EncodedContent, ListMessagesOptions};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let sent = group
        .send(
            EncodedContent {
                r#type: ContentTypeId {
                    authority_id: "example.com".into(),
                    type_id: "b".into(),
                    version_major: 1,
                    version_minor: 0,
                },
                parameters: Default::default(),
                fallback: None,
                content: vec![42],
            },
            None,
        )
        .await?;
    assert!(
        group
            .messages(None)
            .await?
            .iter()
            .any(|message| message.0.id == sent)
    );

    let result = group
        .messages(Some(ListMessagesOptions {
            content_types: Some(vec![ContentTypeId {
                authority_id: "example.com".into(),
                type_id: "a".into(),
                version_major: 1,
                version_minor: 0,
            }]),
            ..Default::default()
        }))
        .await;
    client.end().await?;
    assert!(matches!(result, Err(XmtpError::InvalidArgument(_))));
}
