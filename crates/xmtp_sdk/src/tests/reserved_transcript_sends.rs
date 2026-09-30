use super::*;

// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn reserved_transcript_send_has_stable_input_details() {
    use crate::{ContentTypeId, EncodedContent, ErrorCategory, SendOptions};
    use xmtp_db::{Store, group_message::QueryGroupMessage};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    for type_id in ["group_updated", "group_membership_change"] {
        for optimistic in [false, true] {
            let content = EncodedContent {
                r#type: ContentTypeId {
                    authority_id: "xmtp.org".into(),
                    type_id: type_id.into(),
                    version_major: 7,
                    version_minor: 3,
                },
                parameters: Default::default(),
                fallback: None,
                content: b"fixture".to_vec(),
            };
            let result = group
                .send(
                    content,
                    Some(SendOptions {
                        optimistic,
                        ..Default::default()
                    }),
                )
                .await;
            assert!(matches!(
                result,
                Err(XmtpError::InvalidInput(details))
                    if details.code == "ReservedTranscriptContentType"
                        && matches!(details.category, ErrorCategory::Input)
                        && !details.retryable
            ));
        }
    }
    let template_id = group.inner.prepare_message_for_later_publish(
        b"legacy template",
        false,
        Some("template".into()),
    )?;
    let db = group.inner.context.db();
    let mut stored = db.get_group_message(&template_id)?.unwrap();
    let content = EncodedContent {
        r#type: ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "group_updated".into(),
            version_major: 7,
            version_minor: 3,
        },
        parameters: Default::default(),
        fallback: None,
        content: b"legacy fixture".to_vec(),
    };
    let wire: xmtp_proto::xmtp::mls::message_contents::EncodedContent = content.into();
    let bytes = prost::Message::encode_to_vec(&wire);
    stored.id =
        xmtp_mls::utils::id::calculate_message_id(group.inner.group_id, &bytes, "stored-reserved");
    stored.decrypted_message_bytes = bytes;
    stored.idempotency_key = "stored-reserved".into();
    stored.store(&db)?;
    assert!(matches!(
        group.publish_message(MessageId::from_bytes(&stored.id)?).await,
        Err(XmtpError::InvalidInput(details))
            if details.code == "ReservedTranscriptContentType"
                && matches!(details.category, ErrorCategory::Input)
                && !details.retryable
    ));
    client.end().await?;
}
