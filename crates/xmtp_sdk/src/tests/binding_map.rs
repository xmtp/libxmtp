use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn revoke_all_other_installations_skips_current_installation() {
    let signer = crate::generate_local_signer().await;
    let first = Client::create(signer.clone(), options()).await?;
    assert_eq!(first.inbox_state(true).await?.installations.len(), 1);
    assert!(
        first
            .unsafe_revoke_all_other_installations_signature_request()
            .await?
            .is_none()
    );

    let second = Client::create(signer.clone(), options()).await?;
    let peer = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = first
        .conversations()
        .create_group(vec![peer.inbox_id()], None)
        .await?;
    let solo = first.conversations().create_group(vec![], None).await?;
    second.conversations().sync().await?;
    let revoked_group = second
        .conversations()
        .get_by_id(group.id())
        .await?
        .expect("second installation group");
    let crate::Conversation::Group {
        group: revoked_group,
    } = revoked_group
    else {
        panic!("expected group")
    };
    assert_eq!(first.inbox_state(true).await?.installations.len(), 2);
    let request = first
        .unsafe_revoke_all_other_installations_signature_request()
        .await?
        .expect("the other installation needs a signature");
    request.sign(signer).await?;
    first.unsafe_apply_signature_request(request).await?;
    let state = first.inbox_state(true).await?;
    assert_eq!(state.installations.len(), 1);
    assert_eq!(state.installations[0].id, first.installation_id());
    group.sync().await?;
    revoked_group.sync().await?;
    assert!(
        revoked_group
            .update_name("revoked change".into())
            .await
            .is_err()
    );
    group.update_name("after revoke".into()).await?;
    group.sync().await?;
    assert_eq!(group.state().await?.name, "after revoke");
    peer.conversations().sync().await?;
    let peer_group = peer
        .conversations()
        .get_by_id(group.id())
        .await?
        .expect("peer group");
    let crate::Conversation::Group { group: peer_group } = peer_group else {
        panic!("expected group")
    };
    peer_group.sync().await?;
    solo.update_name("solo after revoke".into()).await?;
    assert_eq!(solo.state().await?.name, "solo after revoke");
    first.end().await?;
    second.end().await?;
    peer.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn catch_up_replays_once_and_preserves_bounded_progress() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = bo
        .conversations()
        .create_group(vec![alix.inbox_id()], None)
        .await?;
    let expected = (0..5).map(|i| format!("owed {i}")).collect::<Vec<_>>();
    for text in &expected {
        group.send_text(text.clone()).await?;
    }

    let _bounded = alix.catch_up_to_live(Some(1)).await;
    let full = alix.catch_up_to_live(None).await?;
    assert!(full.completed);
    let received = alix
        .conversations()
        .get_by_id(group.id())
        .await?
        .expect("catch-up joined the group");
    let crate::Conversation::Group { group: received } = received else {
        panic!("expected a group");
    };
    let actual = received
        .messages(None)
        .await?
        .into_iter()
        .filter_map(|message| match message.0.content {
            MessageContent::Text(text) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    let again = alix.catch_up_to_live(None).await?;
    assert!(again.completed);
    assert_eq!((again.conversations, again.messages), (0, 0));
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn dm_create_is_idempotent_and_peer_ids_survive_lookup() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let first = alix.conversations().create_dm(bo.inbox_id(), None).await?;
    assert_eq!(first.peer_inbox_id(), bo.inbox_id());
    let again = alix.conversations().create_dm(bo.inbox_id(), None).await?;
    assert_eq!(again.id(), first.id());
    let listed = alix.conversations().list(None).await?;
    let dms = listed
        .into_iter()
        .filter_map(|conversation| match conversation {
            crate::Conversation::Dm { dm } => Some(dm),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(dms.len(), 1);
    assert_eq!(dms[0].id(), first.id());
    assert_eq!(dms[0].peer_inbox_id(), bo.inbox_id());
    let groups = alix
        .conversations()
        .list(Some(crate::ListConversationsOptions {
            kind: Some(crate::ConversationKind::Group),
            ..Default::default()
        }))
        .await?;
    assert!(groups.is_empty());
    let only_dms = alix
        .conversations()
        .list(Some(crate::ListConversationsOptions {
            kind: Some(crate::ConversationKind::Dm),
            ..Default::default()
        }))
        .await?;
    assert_eq!(only_dms.len(), 1);

    let alix_summary = alix.conversations().sync_all(None).await?;
    let bo_summary = bo.conversations().sync_all(None).await?;
    assert_eq!(alix_summary.eligible, 1);
    assert_eq!(alix_summary.synced, 1);
    assert_eq!(bo_summary.eligible, 1);
    assert_eq!(bo_summary.synced, 1);
    let from_peer = bo
        .conversations()
        .get_dm_by_inbox_id(alix.inbox_id())
        .await?
        .expect("the peer DM");
    assert_eq!(from_peer.id(), first.id());
    assert_eq!(from_peer.peer_inbox_id(), alix.inbox_id());
    let peer_groups = bo
        .conversations()
        .list(Some(crate::ListConversationsOptions {
            kind: Some(crate::ConversationKind::Group),
            ..Default::default()
        }))
        .await?;
    assert!(peer_groups.is_empty());
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn custom_permission_set_is_converted_and_invalid_set_is_rejected() {
    use crate::{
        CreateGroupOptions, GroupPermissionMode, PermissionPolicy as Policy, PermissionPolicySet,
    };
    let policy_set = PermissionPolicySet {
        add_member: Policy::Allow,
        remove_member: Policy::Deny,
        add_admin: Policy::Admin,
        remove_admin: Policy::Admin,
        update_name: Policy::Admin,
        update_description: Policy::Allow,
        update_image: Policy::Admin,
        update_disappearing: Policy::Admin,
        update_app_data: Policy::SuperAdmin,
    };
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(
            vec![],
            Some(CreateGroupOptions {
                permissions: Some(GroupPermissionMode::Custom {
                    policy_set: policy_set.clone(),
                }),
                ..Default::default()
            }),
        )
        .await?;
    let actual = group.state().await?.permissions.policy_set;
    assert!(matches!(actual.add_member, Policy::Allow));
    assert!(matches!(actual.remove_member, Policy::Deny));
    assert!(matches!(actual.add_admin, Policy::Admin));
    assert!(matches!(actual.remove_admin, Policy::Admin));
    assert!(matches!(actual.update_name, Policy::Admin));
    assert!(matches!(actual.update_description, Policy::Allow));
    assert!(matches!(actual.update_image, Policy::Admin));
    assert!(matches!(actual.update_disappearing, Policy::Admin));
    assert!(matches!(actual.update_app_data, Policy::SuperAdmin));

    let invalid = PermissionPolicySet {
        add_admin: Policy::Allow,
        ..policy_set
    };
    assert!(matches!(
        alix.conversations()
            .create_group(
                vec![],
                Some(CreateGroupOptions {
                    permissions: Some(GroupPermissionMode::Custom {
                        policy_set: invalid
                    }),
                    ..Default::default()
                })
            )
            .await,
        Err(XmtpError::InvalidInput(_))
    ));
    alix.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn backend_url_is_required_and_offline_choice_is_explicit() {
    use xmtp_db::prelude::QueryServerConfiguration;

    assert!(
        crate::Backend::connect(BackendOptions::default())
            .await
            .is_err()
    );
    let unreachable = BackendOptions {
        url: "http://127.0.0.1:1".into(),
        ..Default::default()
    };
    let backend = Arc::new(crate::Backend::connect(unreachable.clone()).await?);
    assert_eq!(backend.options.url, unreachable.url);
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-offline-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.storage = StorageOptions {
        location: StorageLocation::Path(path.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let online = Client::create(signer.clone(), settings.clone()).await?;
    let inbox_id = online.inbox_id();
    let group_id = online
        .conversations()
        .create_group(vec![], None)
        .await?
        .id();
    let stored = online
        .inner
        .context
        .db()
        .server_configuration()?
        .expect("first configuration");
    let first_fetch = stored.fetched_at_ns;
    online.inner.context.db().store_server_configuration(
        &stored.identifier,
        "http://127.0.0.1:2",
        &stored.response,
        first_fetch,
    )?;
    online.end().await?;
    let identity = signer::identity(signer).await?;
    settings.backend = Some(BackendSource::Connected {
        backend: backend.clone(),
    });
    assert!(!settings.allow_offline);
    settings.allow_offline = false;
    assert!(matches!(
        Client::build(identity.clone(), settings.clone(), Some(inbox_id.clone())).await,
        Err(XmtpError::ConfigurationUnavailable(_))
    ));
    settings.allow_offline = ClientOptions::default().allow_offline;
    assert!(matches!(
        Client::build(identity.clone(), settings.clone(), Some(inbox_id.clone())).await,
        Err(XmtpError::ConfigurationUnavailable(_))
    ));
    settings.backend = options().backend;
    let client = Client::build(identity.clone(), settings.clone(), Some(inbox_id.clone())).await?;
    let second_fetch = client
        .inner
        .context
        .db()
        .server_configuration()?
        .expect("refetched configuration")
        .fetched_at_ns;
    assert!(
        second_fetch > first_fetch,
        "default build did not fetch configuration"
    );
    assert_eq!(client.inbox_id(), inbox_id);
    assert!(
        client
            .conversations()
            .list(None)
            .await?
            .iter()
            .any(|conversation| {
                match conversation {
                    crate::Conversation::Group { group } => group.id() == group_id,
                    crate::Conversation::Dm { .. } => false,
                }
            })
    );
    client.end().await?;
    settings.backend = Some(BackendSource::Connected { backend });
    settings.allow_offline = true;
    assert!(matches!(
        Client::build(identity.clone(), settings.clone(), None).await,
        Err(XmtpError::InvalidInput(_))
    ));
    let explicit = Client::build(identity, settings, Some(inbox_id.clone())).await?;
    assert_eq!(explicit.inbox_id(), inbox_id);
    assert!(
        explicit
            .conversations()
            .get_by_id(group_id)
            .await?
            .is_some()
    );
    explicit.end().await?;
    std::fs::remove_file(path)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_create_keeps_one_inbox_id() {
    let signer = crate::generate_local_signer().await;
    let results =
        futures::future::join_all((0..6).map(|_| Client::create(signer.clone(), options()))).await;
    let winners = results.into_iter().flatten().collect::<Vec<_>>();
    assert!(!winners.is_empty());
    let inbox_id = winners[0].inbox_id();
    for client in &winners {
        assert_eq!(client.inbox_id(), inbox_id);
    }
    let state = winners[0].inbox_state(true).await?;
    assert_eq!(state.inbox_id, inbox_id);
    assert_eq!(state.identities.len(), 1);
    assert_eq!(state.installations.len(), winners.len());
    for client in winners {
        client.end().await?;
    }
}

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

    let reference = MessageID::try_from("a".repeat(64))?;
    let reaction = crate::Reaction {
        content: "👍".into(),
        action: crate::ReactionAction::Added,
        schema: crate::ReactionSchema::Unicode,
    };
    let encoded_reaction = ReactionCodec::encode(
        reaction
            .clone()
            .into_proto(reference.clone(), InboxID::try_from("inbox".to_owned())?),
    )?;
    let decoded_reaction = MessageContent::decode(encoded_reaction.encode_to_vec())?;
    assert!(matches!(decoded_reaction, MessageContent::Reaction(value)
        if value.content == reaction.content
            && matches!(value.action, crate::ReactionAction::Added)
            && matches!(value.schema, crate::ReactionSchema::Unicode)));
    let proto_reaction = proto::ReactionV2::decode(encoded_reaction.content.as_slice())?;
    assert_eq!(proto_reaction.reference, reference.0);
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
            if reference_id.0 == "b".repeat(64) && value == "answer"
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
    let id = group.send_text("after rename".into()).await?;
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
    bo_group.send_text("reply".into()).await?;
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
            tokio::task::yield_now().await;
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
    let text_id = group.send_text("typed text".into()).await?;
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
    let first = active.send_text("first".into()).await?;
    let second = active.send_text("second".into()).await?;
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
            group.send_text(format!("message {index}")).await?;
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
        before = Some(last.last_activity_at_ns(None).await?);
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

#[xmtp_common::test(unwrap_try = true)]
fn facade_extended_content_records_keep_nested_fields() {
    use prost::Message as _;
    use xmtp_content_types::{
        ContentCodec,
        actions::{
            Action as CoreAction, ActionStyle as CoreActionStyle, Actions as CoreActions,
            ActionsCodec,
        },
        group_updated::GroupUpdatedCodec,
        intent::{Intent as CoreIntent, IntentCodec},
        leave_request::LeaveRequestCodec,
        text::TextCodec,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupUpdated as CoreGroupUpdated, content_types::LeaveRequest,
    };

    for text in [
        "",
        "Hello 👋 World 🌍! こんにちは 🎉",
        "Line 1\nLine 2\tTabbed\r\nWindows newline",
    ] {
        let encoded = crate::encode_text(text.into())?;
        let decoded = MessageContent::decode(
            xmtp_proto::xmtp::mls::message_contents::EncodedContent::from(encoded).encode_to_vec(),
        )?;
        assert!(matches!(decoded, MessageContent::Text(value) if value == text));
        assert!(
            matches!(MessageContent::decode(TextCodec::encode(text.into())?.encode_to_vec())?, MessageContent::Text(value) if value == text)
        );
    }
    assert!(MessageContent::decode(vec![0xff; 4]).is_err());

    let intent = CoreIntent {
        id: "intent-id".into(),
        action_id: "action-id".into(),
        metadata: Some(serde_json::from_value(
            serde_json::json!({"nested": {"value": 7}}),
        )?),
    };
    assert!(matches!(
        MessageContent::decode(IntentCodec::encode(intent)?.encode_to_vec())?,
        MessageContent::Intent(value)
            if value.id == "intent-id"
                && value.action_id == "action-id"
                && value.metadata_json.as_deref().is_some_and(|json| json.contains("nested"))
    ));
    let actions = CoreActions {
        id: "actions-id".into(),
        description: "choose".into(),
        actions: vec![CoreAction {
            id: "button".into(),
            label: "Confirm".into(),
            image_url: Some("https://example.org/icon".into()),
            style: Some(CoreActionStyle::Primary),
            expires_at: None,
        }],
        expires_at: None,
    };
    assert!(matches!(
        MessageContent::decode(ActionsCodec::encode(actions)?.encode_to_vec())?,
        MessageContent::Actions(value)
            if value.id == "actions-id"
                && value.description == "choose"
                && value.actions.len() == 1
                && value.actions[0].id == "button"
                && value.actions[0].label == "Confirm"
                && matches!(value.actions[0].style, Some(crate::ActionStyle::Primary))
    ));
    use xmtp_proto::xmtp::mls::message_contents::group_updated::{
        Inbox as ProtoInbox, MetadataFieldChange as ProtoFieldChange,
    };
    let update = CoreGroupUpdated {
        initiated_by_inbox_id: "inbox".into(),
        added_inboxes: vec![ProtoInbox {
            inbox_id: "added".into(),
        }],
        removed_inboxes: vec![ProtoInbox {
            inbox_id: "removed".into(),
        }],
        left_inboxes: vec![ProtoInbox {
            inbox_id: "left".into(),
        }],
        metadata_field_changes: vec![ProtoFieldChange {
            field_name: "name".into(),
            old_value: Some("old".into()),
            new_value: Some("new".into()),
        }],
        added_admin_inboxes: vec![ProtoInbox {
            inbox_id: "added-admin".into(),
        }],
        removed_admin_inboxes: vec![ProtoInbox {
            inbox_id: "removed-admin".into(),
        }],
        added_super_admin_inboxes: vec![ProtoInbox {
            inbox_id: "added-super".into(),
        }],
        removed_super_admin_inboxes: vec![ProtoInbox {
            inbox_id: "removed-super".into(),
        }],
    };
    let MessageContent::GroupUpdated(update) =
        MessageContent::decode(GroupUpdatedCodec::encode(update)?.encode_to_vec())?
    else {
        panic!("group update")
    };
    assert_eq!(update.initiated_by_inbox_id.0, "inbox");
    assert_eq!(update.added_inboxes[0].0, "added");
    assert_eq!(update.removed_inboxes[0].0, "removed");
    assert_eq!(update.left_inboxes[0].0, "left");
    assert_eq!(update.metadata_field_changes[0].field_name, "name");
    assert_eq!(
        update.metadata_field_changes[0].old_value.as_deref(),
        Some("old")
    );
    assert_eq!(
        update.metadata_field_changes[0].new_value.as_deref(),
        Some("new")
    );
    assert_eq!(update.added_admin_inboxes[0].0, "added-admin");
    assert_eq!(update.removed_admin_inboxes[0].0, "removed-admin");
    assert_eq!(update.added_super_admin_inboxes[0].0, "added-super");
    assert_eq!(update.removed_super_admin_inboxes[0].0, "removed-super");
    for note in [None, Some(b"leaving".to_vec())] {
        let encoded = LeaveRequestCodec::encode(LeaveRequest {
            authenticated_note: note.clone(),
        })?;
        assert!(matches!(
            MessageContent::decode(encoded.encode_to_vec())?,
            MessageContent::LeaveRequest(value) if value.authenticated_note == note
        ));
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn create_options_keep_disappearing_settings_and_preset_permissions() {
    use crate::{
        CreateDmOptions, CreateGroupOptions, DisappearingSettings, GroupPermissionMode,
        PermissionPolicy as Policy, Timestamp,
    };
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let settings = DisappearingSettings {
        from: Timestamp(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos() as i64,
        ),
        retention_ns: 2_000_000_000,
    };
    let group = alix
        .conversations()
        .create_group(
            vec![bo.inbox_id()],
            Some(CreateGroupOptions {
                permissions: Some(GroupPermissionMode::AllMembers),
                disappearing: Some(settings.clone()),
                ..Default::default()
            }),
        )
        .await?;
    let group_state = group.state().await?;
    assert!(group_state.common.is_disappearing_enabled);
    assert_eq!(
        group_state
            .common
            .disappearing_settings
            .expect("settings")
            .from,
        settings.from
    );
    assert!(matches!(
        group_state.permissions.policy_set.add_member,
        Policy::Allow
    ));
    assert!(matches!(
        group_state.permissions.policy_set.remove_member,
        Policy::Admin
    ));
    assert!(matches!(
        group_state.permissions.policy_set.add_admin,
        Policy::SuperAdmin
    ));
    assert!(matches!(
        group_state.permissions.policy_set.remove_admin,
        Policy::SuperAdmin
    ));
    assert!(matches!(
        group_state.permissions.policy_set.update_name,
        Policy::Allow
    ));
    assert!(matches!(
        group_state.permissions.policy_set.update_description,
        Policy::Allow
    ));
    assert!(matches!(
        group_state.permissions.policy_set.update_image,
        Policy::Allow
    ));
    assert!(matches!(
        group_state.permissions.policy_set.update_disappearing,
        Policy::Admin
    ));
    assert!(matches!(
        group_state.permissions.policy_set.update_app_data,
        Policy::Allow
    ));
    let admins_only = alix
        .conversations()
        .create_group(
            vec![bo.inbox_id()],
            Some(CreateGroupOptions {
                permissions: Some(GroupPermissionMode::AdminOnly),
                name: Some("Group Name".into()),
                image_url: Some("url".into()),
                description: Some("group description".into()),
                disappearing: Some(settings.clone()),
                ..Default::default()
            }),
        )
        .await?;
    let admin_state = admins_only.state().await?;
    assert!(admin_state.common.is_disappearing_enabled);
    assert_eq!(admin_state.name, "Group Name");
    assert_eq!(admin_state.image_url, "url");
    assert_eq!(admin_state.description, "group description");
    let policy = admin_state.permissions.policy_set;
    assert!(matches!(policy.add_member, Policy::Admin));
    assert!(matches!(policy.remove_member, Policy::Admin));
    assert!(matches!(policy.add_admin, Policy::SuperAdmin));
    assert!(matches!(policy.remove_admin, Policy::SuperAdmin));
    assert!(matches!(policy.update_name, Policy::Admin));
    assert!(matches!(policy.update_description, Policy::Admin));
    assert!(matches!(policy.update_image, Policy::Admin));
    assert!(matches!(policy.update_disappearing, Policy::Admin));
    assert!(matches!(policy.update_app_data, Policy::Admin));
    let zero_from = alix
        .conversations()
        .create_group(
            vec![bo.inbox_id()],
            Some(CreateGroupOptions {
                disappearing: Some(DisappearingSettings {
                    from: Timestamp(0),
                    retention_ns: 5,
                }),
                ..Default::default()
            }),
        )
        .await?;
    let zero_state = zero_from.state().await?;
    assert!(!zero_state.common.is_disappearing_enabled);
    assert_eq!(
        zero_state
            .common
            .disappearing_settings
            .expect("zero settings")
            .retention_ns,
        5
    );
    let dm = alix
        .conversations()
        .create_dm(
            bo.inbox_id(),
            Some(CreateDmOptions {
                disappearing: Some(settings),
            }),
        )
        .await?;
    let dm_state = dm.state().await?;
    assert!(dm_state.is_disappearing_enabled);
    assert_eq!(
        dm_state
            .disappearing_settings
            .expect("DM settings")
            .retention_ns,
        2_000_000_000
    );
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn can_message_changes_after_peer_registration() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo_signer = crate::generate_local_signer().await;
    let bo_identity = bo_signer.identity().await?;
    let before = alix.can_message(vec![bo_identity.clone()]).await?;
    assert_eq!(before.len(), 1);
    assert!(!before[0].can_message);
    let bo = Client::create(bo_signer, options()).await?;
    let after = alix.can_message(vec![bo_identity]).await?;
    assert_eq!(after.len(), 1);
    assert!(after[0].can_message);
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn member_consent_is_visible_in_group_member_record() {
    use crate::{ConsentEntity, ConsentRecord, ConsentState};
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    let entity = ConsentEntity::Inbox {
        inbox_id: bo.inbox_id(),
    };
    alix.preferences()
        .set_consent_states(vec![ConsentRecord {
            entity: entity.clone(),
            state: ConsentState::Allowed,
        }])
        .await?;
    assert!(matches!(
        alix.preferences().consent_state(entity).await?,
        ConsentState::Allowed
    ));
    let member = group
        .members()
        .await?
        .into_iter()
        .find(|member| member.inbox_id == bo.inbox_id())
        .expect("peer member");
    assert!(matches!(member.consent_state, ConsentState::Allowed));
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn storage_key_rejects_wrong_key_for_existing_database() {
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-binding-key-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let signer = crate::generate_local_signer().await;
    let mut first_options = options();
    first_options.storage = StorageOptions {
        location: StorageLocation::Path(path.to_string_lossy().into_owned()),
        encryption_key: Some(vec![7; 32]),
        ..Default::default()
    };
    let first = Client::create(signer.clone(), first_options.clone()).await?;
    first.end().await?;
    let second = Client::create(
        signer,
        ClientOptions {
            storage: StorageOptions {
                encryption_key: Some(vec![8; 32]),
                ..first_options.storage
            },
            ..first_options
        },
    )
    .await;
    assert!(second.is_err(), "a different database key must fail");
    std::fs::remove_file(path)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn passkey_signature_associates_identity_through_facade() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let passkey = xmtp_id::utils::passkey::PasskeyUser::new().await;
    let identity = PublicIdentity::from(passkey.get_identifier()?);
    let request = alix
        .unsafe_add_account_signature_request(identity.clone(), false)
        .await?;
    let UnverifiedSignature::Passkey(signature) = passkey.sign(&request.signature_text().await)?
    else {
        panic!("passkey fixture returned the wrong signature kind");
    };
    request
        .add_signature(Signature::Passkey {
            signature: signature.signature,
            public_key: signature.public_key,
            authenticator_data: signature.authenticator_data,
            client_data_json: signature.client_data_json,
        })
        .await?;
    alix.unsafe_apply_signature_request(request).await?;
    let state = alix.inbox_state(true).await?;
    assert!(
        state
            .identities
            .iter()
            .any(|value| value.identifier == identity.identifier
                && matches!(value.kind, PublicIdentityKind::Passkey))
    );
    alix.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn new_installation_can_find_existing_dm() {
    let signer = crate::generate_local_signer().await;
    let sync_options = ClientOptions {
        device_sync: true,
        ..options()
    };
    let first = Client::create(signer.clone(), sync_options.clone()).await?;
    let peer = Client::create(crate::generate_local_signer().await, options()).await?;
    let dm = first
        .conversations()
        .create_dm(peer.inbox_id(), None)
        .await?;
    first.conversations().sync().await?;
    peer.conversations().sync().await?;
    let second = Client::create(signer, sync_options).await?;
    assert!(second.conversations().list(None).await?.is_empty());
    dm.send_text("new installation delivery".into()).await?;
    first.conversations().sync().await?;
    second.catch_up_to_live(None).await?;
    let found = second
        .conversations()
        .get_dm_by_inbox_id(peer.inbox_id())
        .await?
        .expect("new installation can find the DM");
    assert_eq!(found.id(), dm.id());
    assert_eq!(found.peer_inbox_id(), peer.inbox_id());
    first.end().await?;
    second.end().await?;
    peer.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn removed_member_does_not_receive_later_group_message() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    let bo_group = xmtp_common::time::timeout(Duration::from_secs(10), async {
        loop {
            bo.conversations().sync().await?;
            if let Some(found) = bo.conversations().get_by_id(group.id()).await? {
                break Ok::<_, XmtpError>(found);
            }
            xmtp_common::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    let crate::Conversation::Group { group: bo_group } = bo_group else {
        panic!("expected group")
    };
    group.remove_members(vec![bo.inbox_id()]).await?;
    group.send_text("only current members".into()).await?;
    xmtp_common::time::timeout(Duration::from_secs(10), async {
        loop {
            let _ = bo_group.sync().await;
            if !bo_group.state().await?.common.is_active {
                break Ok::<(), XmtpError>(());
            }
            xmtp_common::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    assert!(!bo_group.state().await?.common.is_active);
    assert!(!bo_group.messages(None).await?.iter().any(|message| {
        matches!(&message.0.content, MessageContent::Text(value) if value == "only current members")
    }));
    alix.end().await?;
    bo.end().await?;
}
