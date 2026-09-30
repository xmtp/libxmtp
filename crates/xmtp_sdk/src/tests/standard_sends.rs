use super::*;

// verifies: CTYPE-014
#[xmtp_common::test(unwrap_try = true)]
async fn standard_codec_bytes_match_typed_send_wire_bytes() {
    use crate::StandardContent;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let mut sent = 0;
    for (value, expected) in crate::content::pure_codec_tests::standard_codec_samples()? {
        let id = match value {
            StandardContent::Text(text) => group.send_text(text, None).await?,
            StandardContent::Markdown(markdown) => group.send_markdown(markdown, None).await?,
            StandardContent::Reaction {
                reference,
                reference_inbox_id,
                reaction,
            } => {
                group
                    .send_reaction(reference, reference_inbox_id, reaction, None)
                    .await?
            }
            StandardContent::Reply {
                reference,
                reference_inbox_id,
                content,
            } => {
                group
                    .send_reply(reference, reference_inbox_id, content, None)
                    .await?
            }
            StandardContent::ReadReceipt => group.send_read_receipt(None).await?,
            StandardContent::Attachment(attachment) => {
                group.send_attachment(attachment, None).await?
            }
            StandardContent::RemoteAttachment(attachment) => {
                group.send_remote_attachment(attachment, None).await?
            }
            StandardContent::MultiRemoteAttachment(attachment) => {
                group.send_multi_remote_attachment(attachment, None).await?
            }
            StandardContent::TransactionReference(reference) => {
                group.send_transaction_reference(reference, None).await?
            }
            StandardContent::WalletSendCalls(calls) => {
                group.send_wallet_send_calls(calls, None).await?
            }
            StandardContent::Actions(actions) => group.send_actions(actions, None).await?,
            StandardContent::Intent(intent) => group.send_intent(intent, None).await?,
            StandardContent::GroupUpdated(_)
            | StandardContent::DeleteMessage { .. }
            | StandardContent::LeaveRequest(_) => continue,
        };
        let stored = client
            .conversations()
            .get_message_by_id(id)
            .await?
            .expect("sent message");
        let expected_type = expected.r#type.expect("content type");
        let actual_type = &stored
            .0
            .encoded
            .as_ref()
            .expect("usable encoded content")
            .r#type;
        assert_eq!(actual_type.authority_id, expected_type.authority_id);
        assert_eq!(actual_type.type_id, expected_type.type_id);
        assert_eq!(actual_type.version_major, expected_type.version_major);
        assert_eq!(actual_type.version_minor, expected_type.version_minor);
        assert_eq!(
            stored
                .0
                .encoded
                .as_ref()
                .expect("usable encoded content")
                .parameters,
            expected.parameters
        );
        assert_eq!(
            stored
                .0
                .encoded
                .as_ref()
                .expect("usable encoded content")
                .fallback,
            expected.fallback
        );
        assert_eq!(
            stored
                .0
                .encoded
                .as_ref()
                .expect("usable encoded content")
                .content,
            expected.content
        );
        sent += 1;
    }
    assert_eq!(sent, 12);
    client.end().await?;
}

// verifies: CTYPE-010
#[xmtp_common::test(unwrap_try = true)]
async fn message_action_push_defaults_follow_content_type() {
    use crate::{Reaction, ReactionAction, ReactionSchema, SendOptions};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let reference = group.send_text("reference".into(), None).await?;
    let reaction = Reaction {
        content: "👍".into(),
        action: ReactionAction::Added,
        schema: ReactionSchema::Unicode,
    };
    for options in [None, Some(SendOptions::default())] {
        let id = client
            .conversations()
            .react_to_message(reference.clone(), reaction.clone(), options)
            .await?;
        assert!(!client.inner.message(id.to_bytes()?)?.should_push);
    }
    let raw_text = group.send(crate::encode_text("raw".into())?, None).await?;
    assert!(client.inner.message(raw_text.to_bytes()?)?.should_push);
    let overridden = client
        .conversations()
        .react_to_message(
            reference,
            reaction,
            Some(SendOptions {
                should_push: Some(true),
                ..Default::default()
            }),
        )
        .await?;
    assert!(client.inner.message(overridden.to_bytes()?)?.should_push);
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn reaction_with_compression_only_does_not_push() {
    use crate::{Compression, Reaction, ReactionAction, ReactionSchema, SendOptions};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let reference = group.send_text("reference".into(), None).await?;
    let reaction = group
        .send_reaction(
            reference,
            Some(client.inbox_id()),
            Reaction {
                content: "👍".into(),
                action: ReactionAction::Added,
                schema: ReactionSchema::Unicode,
            },
            Some(SendOptions {
                should_push: None,
                compression: Some(Compression::Gzip),
                ..Default::default()
            }),
        )
        .await?;
    let stored = client.inner.message(reaction.to_bytes()?)?;
    assert!(!stored.should_push);
    client.end().await?;
}
