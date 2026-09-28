use super::*;

fn catalogue_push_default(content_type: &ContentTypeId) -> bool {
    !matches!(
        (
            content_type.authority_id.as_str(),
            content_type.type_id.as_str(),
            content_type.version_major,
        ),
        ("xmtp.org", "reaction", 2)
            | ("xmtp.org", "readReceipt", 1)
            | ("xmtp.org", "group_updated", 1)
            | ("xmtp.org", "group_membership_change", 1)
            | ("xmtp.org", "leave_request", 1)
            | ("xmtp.org", "deleteMessage", 1)
            | ("xmtp.org", "editMessage", 1)
    )
}

pub(super) async fn send_standard(
    group: MlsGroup<xmtp_mls::MlsContext>,
    value: StandardContent,
    options: Option<SendOptions>,
) -> Result<MessageId, XmtpError> {
    send_encoded(
        group,
        crate::encode_standard(value)?,
        options.unwrap_or_default(),
    )
    .await
}

pub(super) async fn send_encoded(
    group: MlsGroup<xmtp_mls::MlsContext>,
    content: EncodedContent,
    options: SendOptions,
) -> Result<MessageId, XmtpError> {
    require_content_type(&content)?;
    on_sdk_worker(group.context.clone(), async move {
        // Build the send future on the worker. Swift cooperative threads have
        // a small stack and cannot hold this nested MLS future before spawn.
        Box::pin(async move {
            // implements: CTYPE-010
            let should_push = options
                .should_push
                .unwrap_or_else(|| catalogue_push_default(&content.r#type));
            let content =
                compress_if_requested(content.into(), options.compression.map(Into::into))
                    .map_err(XmtpError::unknown)?;
            let bytes = encoded_content_to_bytes(content);
            let opts = SendMessageOpts {
                should_push,
                idempotency_key: options.idempotency_key,
            };
            let id = if options.optimistic {
                group
                    .send_message_optimistic(&bytes, opts)
                    .map_err(XmtpError::unknown)?
            } else {
                group
                    .send_message(&bytes, opts)
                    .await
                    .map_err(XmtpError::unknown)?
            };
            MessageId::from_bytes(&id)
        })
        .await
    })
    .await
}

pub(super) fn require_content_type(content: &EncodedContent) -> Result<(), XmtpError> {
    if content.r#type.authority_id.is_empty() || content.r#type.type_id.is_empty() {
        return Err(XmtpError::invalid("content type identifier is empty"));
    }
    Ok(())
}

pub(super) fn parent_stored(
    group: &MlsGroup<xmtp_mls::MlsContext>,
    message: &DecodedMessage,
) -> Result<Option<StoredGroupMessage>, XmtpError> {
    let CoreMessageBody::Reply(reply) = &message.content else {
        return Ok(None);
    };
    let Some(parent) = &reply.in_reply_to else {
        return Ok(None);
    };
    group
        .context
        .db()
        .get_group_message(&parent.metadata.id)
        .map_err(XmtpError::unknown)
}

pub(crate) fn lift_history_messages(
    enriched: Vec<EnrichedStoredMessage>,
    client_key: u64,
) -> Vec<Message> {
    enriched
        .into_iter()
        .filter_map(|enriched| {
            let message_id = enriched.stored.id.clone();
            match Message::from_enriched(
                enriched.stored,
                enriched.decoded,
                enriched.parent_stored,
                client_key,
            ) {
                Ok(message) => Some(message),
                Err(err) => {
                    tracing::warn!(
                        message_id = %hex::encode(&message_id),
                        error = %err,
                        "skipping stored message that failed to convert"
                    );
                    None
                }
            }
        })
        .collect()
}

pub(crate) fn query_content_types(
    values: Vec<ContentTypeId>,
) -> Result<Vec<xmtp_db::group_message::ContentType>, XmtpError> {
    values
        .into_iter()
        .map(|value| {
            let kind = xmtp_db::group_message::ContentType::from_identifier(
                &value.authority_id,
                &value.type_id,
                value.version_major,
            );
            if matches!(kind, xmtp_db::group_message::ContentType::Unknown) {
                Err(XmtpError::invalid_argument(
                    "content type cannot be used as a message filter",
                ))
            } else {
                Ok(kind)
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "push_default_tests.rs"]
mod push_default_tests;
