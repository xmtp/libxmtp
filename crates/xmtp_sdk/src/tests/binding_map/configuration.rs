use super::*;

#[xmtp_common::test(unwrap_try = true)]
fn facade_content_records_preserve_codec_fields() {
    use prost::Message as _;
    use xmtp_content_types::{
        ContentCodec,
        attachment::{Attachment as CoreAttachment, AttachmentCodec},
        multi_remote_attachment::MultiRemoteAttachmentCodec,
        reaction::ReactionCodec,
        read_receipt::{ReadReceipt, ReadReceiptCodec},
        remote_attachment::{RemoteAttachment as CoreRemoteAttachment, RemoteAttachmentCodec},
        reply::{Reply, ReplyCodec},
        text::TextCodec,
        transaction_reference::{
            TransactionReference as CoreTransactionReference, TransactionReferenceCodec,
        },
    };
    use xmtp_proto::xmtp::mls::message_contents::content_types as proto;

    let reference = MessageId::try_from("a".repeat(64))?;
    let reaction = crate::Reaction {
        content: "👍".into(),
        action: crate::ReactionAction::Added,
        schema: crate::ReactionSchema::Unicode,
    };
    let encoded_reaction = ReactionCodec::encode(
        reaction
            .clone()
            .into_proto(reference.checked()?.to_owned(), "inbox".to_owned()),
    )?;
    let decoded_reaction = MessageContent::decode(encoded_reaction.encode_to_vec())?;
    assert!(
        matches!(decoded_reaction, MessageContent::Reaction { reaction: value, .. }
        if value.content == reaction.content
            && matches!(value.action, crate::ReactionAction::Added)
            && matches!(value.schema, crate::ReactionSchema::Unicode))
    );
    let proto_reaction = proto::ReactionV2::decode(encoded_reaction.content.as_slice())?;
    assert_eq!(proto_reaction.reference, reference.checked()?);
    assert_eq!(proto_reaction.reference_inbox_id, "inbox");

    let attachment = CoreAttachment {
        filename: Some("a.txt".into()),
        mime_type: "text/plain".into(),
        content: b"attachment".to_vec(),
    };
    assert!(matches!(
        MessageContent::decode(AttachmentCodec::encode(attachment)?.encode_to_vec())?,
        MessageContent::Attachment(value)
            if value.filename.as_deref() == Some("a.txt")
                && value.mime_type == "text/plain"
                && value.content == b"attachment"
    ));

    let remote = CoreRemoteAttachment {
        url: "https://example.org/a".into(),
        content_digest: "digest".into(),
        secret: vec![1, 2],
        salt: vec![3, 4],
        nonce: vec![5, 6],
        scheme: "https".into(),
        content_length: Some(12),
        filename: Some("a.txt".into()),
    };
    assert!(matches!(
        MessageContent::decode(RemoteAttachmentCodec::encode(remote.clone())?.encode_to_vec())?,
        MessageContent::RemoteAttachment(value)
            if value.url == remote.url
                && value.content_digest == remote.content_digest
                && value.secret == remote.secret
                && value.salt == remote.salt
                && value.nonce == remote.nonce
                && value.scheme == remote.scheme
                && value.content_length == remote.content_length
                && value.filename == remote.filename
    ));
    let multi = proto::MultiRemoteAttachment {
        attachments: vec![remote.clone(), remote],
    };
    assert!(matches!(
        MessageContent::decode(MultiRemoteAttachmentCodec::encode(multi)?.encode_to_vec())?,
        MessageContent::MultiRemoteAttachment(value)
            if value.attachments.len() == 2
                && value.attachments[0].content_digest == "digest"
                && value.attachments[1].url == "https://example.org/a"
    ));

    let transaction = CoreTransactionReference {
        namespace: Some("eip155".into()),
        network_id: "1".into(),
        reference: "0xabc".into(),
        metadata: Some(
            xmtp_content_types::transaction_reference::TransactionMetadata {
                transaction_type: "transfer".into(),
                currency: "ETH".into(),
                amount: 0.42,
                decimals: 18,
                from_address: "0xfrom".into(),
                to_address: "0xto".into(),
            },
        ),
    };
    assert!(matches!(
        MessageContent::decode(TransactionReferenceCodec::encode(transaction)?.encode_to_vec())?,
        MessageContent::TransactionReference(value)
            if value.reference == "0xabc"
                && value.metadata.as_ref().is_some_and(|metadata| metadata.currency == "ETH")
    ));
    assert!(matches!(
        MessageContent::decode(ReadReceiptCodec::encode(ReadReceipt {})?.encode_to_vec())?,
        MessageContent::ReadReceipt
    ));
    let reply = Reply {
        reference: "b".repeat(64),
        reference_inbox_id: Some("inbox".into()),
        content: TextCodec::encode("answer".into())?,
    };
    assert!(matches!(
        MessageContent::decode(ReplyCodec::encode(reply)?.encode_to_vec())?,
        MessageContent::Reply { reference_id, body: crate::MessageBody::Text(value) }
            if reference_id.checked().ok() == Some(&*"b".repeat(64)) && value == "answer"
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn reader_survives_group_name_update_without_fork() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    bo.inner.sync_welcomes().await?;
    let bo_group = crate::Group::from_core(bo.inner.group(&group.inner.group_id)?, bo.key).await?;
    let reader = bo_group.message_reader().await?;
    group.update_name("renamed".into()).await?;
    let id = group.send_text("after rename".into(), None).await?;
    let delivered = xmtp_common::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Some(message) = reader.next().await?
                && message.0.id == id
            {
                break Ok::<_, XmtpError>(message);
            }
        }
    })
    .await??;
    assert!(matches!(delivered.0.content, MessageContent::Text(value) if value == "after rename"));
    reader.end().await?;
    bo_group.sync().await?;
    assert_eq!(bo_group.id(), group.id());
    assert_eq!(bo_group.state().await?.name, "renamed");
    let history = bo_group.messages(None).await?;
    assert!(history.iter().any(|message| matches!(&message.0.content,
        MessageContent::GroupUpdated(update)
            if update.metadata_field_changes.iter().any(|field| field.field_name == "group_name" && field.new_value.as_deref() == Some("renamed")))));
    assert!(history.iter().any(|message| message.0.id == id));
    bo_group.send_text("reply".into(), None).await?;
    group.sync().await?;
    assert_eq!(group.id(), bo_group.id());
    assert!(group.messages(None).await?.iter().any(|message| {
        matches!(&message.0.content, MessageContent::Text(text) if text == "reply")
    }));
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn dm_consent_is_read_from_the_dm() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let alix_dm = alix.conversations().create_dm(bo.inbox_id(), None).await?;
    assert!(matches!(
        alix_dm.state().await?.consent_state,
        crate::ConsentState::Allowed
    ));
    bo.conversations().sync_all(None).await?;
    let bo_dm = bo
        .conversations()
        .get_dm_by_inbox_id(alix.inbox_id())
        .await?
        .expect("peer DM");
    assert!(matches!(
        bo_dm.state().await?.consent_state,
        crate::ConsentState::Unknown
    ));
    alix_dm
        .update_consent_state(crate::ConsentState::Denied)
        .await?;
    assert!(matches!(
        alix_dm.state().await?.consent_state,
        crate::ConsentState::Denied
    ));
    bo_dm
        .update_consent_state(crate::ConsentState::Allowed)
        .await?;
    assert!(matches!(
        bo_dm.state().await?.consent_state,
        crate::ConsentState::Allowed
    ));

    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn dm_duplicate_lookup_finds_the_other_conversation() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let first = alix.conversations().create_dm(bo.inbox_id(), None).await?;
    let second = bo.conversations().create_dm(alix.inbox_id(), None).await?;
    assert_ne!(first.id(), second.id());
    let duplicates = xmtp_common::time::timeout(Duration::from_secs(10), async {
        loop {
            alix.conversations().sync_all(None).await?;
            let duplicates = first.duplicate_dms().await?;
            if duplicates.iter().any(|dm| dm.id() == second.id()) {
                break Ok::<_, XmtpError>(duplicates);
            }
            xmtp_common::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].id(), second.id());
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn group_creation_with_members_and_content_type_filters() {
    use crate::{CreateGroupOptions, ListMessagesOptions};
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(
            vec![bo.inbox_id()],
            Some(CreateGroupOptions {
                name: Some("mapped group".into()),
                description: Some("mapped description".into()),
                ..Default::default()
            }),
        )
        .await?;
    assert_eq!(group.state().await?.name, "mapped group");
    assert_eq!(group.state().await?.description, "mapped description");
    let members = group.members().await?;
    assert!(
        members
            .iter()
            .any(|member| member.inbox_id == bo.inbox_id())
    );
    let debug = group.debug_info().await?;
    assert_eq!(debug.epoch, 1);
    assert!(!debug.maybe_forked);
    assert!(debug.fork_details.is_empty());
    let text_id = group.send_text("typed text".into(), None).await?;
    let text_type = crate::encode_text("sample".into())?.r#type;
    let text_only = group
        .messages(Some(ListMessagesOptions {
            content_types: Some(vec![text_type.clone()]),
            ..Default::default()
        }))
        .await?;
    assert_eq!(text_only.len(), 1);
    assert_eq!(text_only[0].0.id, text_id);
    let without_text = group
        .messages(Some(ListMessagesOptions {
            exclude_content_types: Some(vec![text_type]),
            ..Default::default()
        }))
        .await?;
    assert!(without_text.iter().all(|message| message.0.id != text_id));
    bo.conversations().sync_all(None).await?;
    let bo_group = bo
        .conversations()
        .get_by_id(group.id())
        .await?
        .expect("member sees group");
    assert!(matches!(bo_group, crate::Conversation::Group { .. }));
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn listed_conversations_keep_last_message_and_empty_groups() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let empty = client.conversations().create_group(vec![], None).await?;
    let active = client.conversations().create_group(vec![], None).await?;
    let first = active.send_text("first".into(), None).await?;
    let second = active.send_text("second".into(), None).await?;
    assert_ne!(first, second);
    let listed = client.conversations().list_groups(None).await?;
    assert_eq!(listed.len(), 2);
    let empty_listed = listed
        .iter()
        .find(|group| group.id() == empty.id())
        .expect("empty group");
    assert!(empty_listed.last_message().await?.is_none());
    let active_listed = listed
        .iter()
        .find(|group| group.id() == active.id())
        .expect("active group");
    let last = active_listed.last_message().await?.expect("latest message");
    assert_eq!(last.0.id, second);
    assert!(matches!(last.0.content, MessageContent::Text(value) if value == "second"));
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn conversation_list_limit_and_activity_cursor_cover_all_groups() {
    use crate::{ConversationOrder, ListConversationsOptions};
    use std::collections::HashSet;
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let mut groups = Vec::new();
    for index in 0..6 {
        let group = client.conversations().create_group(vec![], None).await?;
        if index % 2 == 0 {
            group.send_text(format!("message {index}"), None).await?;
        }
        groups.push(group);
    }
    let mut seen = HashSet::new();
    let mut before = None;
    loop {
        let page = client
            .conversations()
            .list_groups(Some(ListConversationsOptions {
                limit: Some(2),
                order_by: Some(ConversationOrder::LastActivity),
                last_activity_before: before,
                ..Default::default()
            }))
            .await?;
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= 2, "the façade must keep the requested limit");
        for group in &page {
            assert!(
                seen.insert(group.id()),
                "conversation appeared on two pages"
            );
        }
        let last = page.last().expect("page is not empty");
        before = Some(last.last_activity_at(None).await?);
        if page.len() < 2 {
            break;
        }
    }
    assert_eq!(seen.len(), groups.len());
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn unsigned_signature_request_cannot_register() {
    let mut settings = options();
    settings.registration.auto = false;
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    let request = client
        .unsafe_create_inbox_signature_request()
        .await?
        .expect("new inbox request");
    assert!(
        client
            .unsafe_apply_signature_request(request)
            .await
            .is_err()
    );
    assert!(!client.is_registered().await?);
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn static_revoke_checks_recovery_signer_and_removes_target() {
    let signer = crate::generate_local_signer().await;
    let first = Client::create(signer.clone(), options()).await?;
    let second = Client::create(signer.clone(), options()).await?;
    let backend = options().backend.expect("backend");
    let target = second.installation_id();
    let wrong = crate::generate_local_signer().await;
    assert!(
        crate::static_helpers::revoke_installations_with_backend(
            backend.clone(),
            wrong,
            first.inbox_id(),
            vec![target.clone()]
        )
        .await
        .is_err()
    );
    assert_eq!(first.inbox_state(true).await?.installations.len(), 2);
    crate::static_helpers::revoke_installations_with_backend(
        backend,
        signer,
        first.inbox_id(),
        vec![target],
    )
    .await?;
    let state = first.inbox_state(true).await?;
    assert_eq!(state.installations.len(), 1);
    assert_eq!(state.installations[0].id, first.installation_id());
    first.end().await?;
    second.end().await?;
}
