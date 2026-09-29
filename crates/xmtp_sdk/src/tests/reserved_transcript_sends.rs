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

// verifies: GMOD-035, SEND-019
#[xmtp_common::test(unwrap_try = true)]
async fn bulk_publish_keeps_unknown_reserved_outcome_in_public_details() {
    use crate::{ContentTypeId, EncodedContent, ErrorCategory, SendOptions};
    use xmtp_db::group_message::{DeliveryStatus, QueryGroupMessage};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    group.inner.key_update().await?;
    group.inner.update_installations().await?;
    let content = EncodedContent {
        r#type: ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "group_updated".into(),
            version_major: 7,
            version_minor: 3,
        },
        parameters: Default::default(),
        fallback: None,
        content: b"prior-client fixture".to_vec(),
    };
    let wire: xmtp_proto::xmtp::mls::message_contents::EncodedContent = content.into();
    let (reserved_id, _) = group
        .inner
        .prepare_reserved_attempt_for_test(&prost::Message::encode_to_vec(&wire), "sdk-bulk")
        .await?;
    let allowed = group
        .send_text(
            "later allowed".into(),
            Some(SendOptions {
                optimistic: true,
                ..Default::default()
            }),
        )
        .await?;

    assert!(matches!(
        group.publish_messages().await,
        Err(XmtpError::Unknown(details))
            if details.code == "SendOutcomeUnknown"
                && matches!(details.category, ErrorCategory::Conversation)
                && details.retryable
    ));
    let db = group.inner.context.db();
    assert_eq!(
        db.get_group_message(&allowed.to_bytes()?)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Published
    );
    assert_eq!(
        db.get_group_message(&reserved_id)?.unwrap().delivery_status,
        DeliveryStatus::Unpublished
    );
    client.end().await?;
}

// verifies: GMOD-035, SEND-019
#[xmtp_common::test(unwrap_try = true)]
async fn bulk_publish_exposes_selected_terminal_ordered_rejection() {
    use crate::{ContentTypeId, EncodedContent, ErrorCategory, SendOptions};
    use xmtp_db::group_message::{DeliveryStatus, QueryGroupMessage};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    group.inner.key_update().await?;
    group.inner.update_installations().await?;
    let content = EncodedContent {
        r#type: ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "group_updated".into(),
            version_major: 7,
            version_minor: 3,
        },
        parameters: Default::default(),
        fallback: None,
        content: b"prior-client fixture".to_vec(),
    };
    let wire: xmtp_proto::xmtp::mls::message_contents::EncodedContent = content.into();
    let (reserved_id, reserved_intent) = group
        .inner
        .prepare_reserved_attempt_for_test(&prost::Message::encode_to_vec(&wire), "sdk-rejected")
        .await?;
    group
        .inner
        .publish_future_epoch_reserved_echo_for_test(reserved_intent)
        .await?;
    let allowed = group
        .send_text(
            "later allowed".into(),
            Some(SendOptions {
                optimistic: true,
                ..Default::default()
            }),
        )
        .await?;

    let result = group.publish_messages().await;
    assert!(
        matches!(result, Err(XmtpError::Unknown(ref details))
        if details.code == "impossible_future_epoch"
            && matches!(details.category, ErrorCategory::Conversation)
            && !details.retryable),
        "{result:?}"
    );
    let db = group.inner.context.db();
    assert_eq!(
        db.get_group_message(&allowed.to_bytes()?)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Published
    );
    assert_eq!(
        db.get_group_message(&reserved_id)?.unwrap().delivery_status,
        DeliveryStatus::Failed
    );
    let exact = group
        .inner
        .publish_stored_message(&reserved_id)
        .await
        .unwrap_err();
    let exact = XmtpError::from_group(exact);
    assert!(
        matches!(exact, XmtpError::Unknown(ref details)
        if details.code == "impossible_future_epoch"
            && matches!(details.category, ErrorCategory::Conversation)
            && !details.retryable),
        "{exact:?}"
    );
    client.end().await?;
}
