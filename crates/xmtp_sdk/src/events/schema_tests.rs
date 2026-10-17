use super::*;

// verifies: EVENT-024
#[xmtp_common::test(unwrap_try = true)]
fn event_payloads_keep_core_identifier_bytes_and_cause() {
    let group = vec![0xa5; 16];
    let message = vec![0xb6; 32];
    let installation = vec![0xc7; 32];
    let joined = ClientEvent::from_core(core::ClientEvent::ConversationJoined(
        core::ConversationJoined {
            group_id: group.clone(),
            conversation_type: core::ConversationType::Group,
            origin: core::JoinOrigin::Created,
            adder_inbox_id: None,
        },
    ));
    let ClientEvent::ConversationJoined {
        conversation_joined,
    } = joined
    else {
        panic!("joined payload")
    };
    assert_eq!(
        conversation_joined.group_id, group,
        "event group bytes changed"
    );
    let received =
        ClientEvent::from_core(core::ClientEvent::MessageReceived(core::MessageReceived {
            group_id: group.clone(),
            message_id: message.clone(),
            sender_inbox_id: "inbox".into(),
            content_type: Some(core::ContentTypeId {
                authority_id: "xmtp.org".into(),
                type_id: "text".into(),
                version_major: 1,
            }),
        }));
    let ClientEvent::MessageReceived { message_received } = received else {
        panic!("message payload")
    };
    assert_eq!(message_received.group_id, group);
    assert_eq!(message_received.message_id, message);
    let content = message_received.content_type.expect("content type");
    assert_eq!(
        (
            content.authority_id.as_str(),
            content.type_id.as_str(),
            content.version_major
        ),
        ("xmtp.org", "text", 1)
    );
    let identity = ClientEvent::from_core(core::ClientEvent::IdentityRegistered(
        core::IdentityRegistered {
            inbox_id: "inbox".into(),
            installation_key: installation.clone(),
        },
    ));
    let ClientEvent::IdentityRegistered {
        identity_registered,
    } = identity
    else {
        panic!("identity payload")
    };
    assert_eq!(identity_registered.installation_key, installation);
    let failed: AttachmentFailed = core::AttachmentFailed {
        attachment_key: "key".into(),
        url: "https://example.invalid".into(),
        content_digest: "digest".into(),
        cause: "digest_mismatch".into(),
    }
    .into();
    assert_eq!(failed.cause, "digest_mismatch");
}
