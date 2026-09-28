use super::*;

#[xmtp_common::test(unwrap_try = true)]
fn standard_content_types_decode_to_records() {
    use prost::Message as _;
    use xmtp_content_types::{
        ContentCodec,
        actions::{Action, Actions, ActionsCodec},
        attachment::{Attachment, AttachmentCodec},
        group_updated::GroupUpdatedCodec,
        intent::{Intent, IntentCodec},
        leave_request::LeaveRequestCodec,
        remote_attachment::{RemoteAttachment, RemoteAttachmentCodec},
        transaction_reference::{TransactionReference, TransactionReferenceCodec},
        wallet_send_calls::{WalletSendCalls, WalletSendCallsCodec},
    };
    use xmtp_proto::xmtp::mls::message_contents::{GroupUpdated, content_types::LeaveRequest};

    let attachment = Attachment {
        filename: Some("file.txt".into()),
        mime_type: "text/plain".into(),
        content: b"data".to_vec(),
    };
    assert!(
        matches!(MessageContent::decode(AttachmentCodec::encode(attachment)?.encode_to_vec())?, MessageContent::Attachment(value) if value.content == b"data")
    );
    let remote = RemoteAttachment {
        url: "https://example.org/file".into(),
        content_digest: "abc".into(),
        secret: vec![1],
        salt: vec![2],
        nonce: vec![3],
        scheme: "https".into(),
        content_length: Some(1),
        filename: None,
    };
    assert!(
        matches!(MessageContent::decode(RemoteAttachmentCodec::encode(remote)?.encode_to_vec())?, MessageContent::RemoteAttachment(value) if value.url == "https://example.org/file")
    );
    let transaction = TransactionReference {
        namespace: None,
        network_id: "1".into(),
        reference: "0xabc".into(),
        metadata: None,
    };
    assert!(
        matches!(MessageContent::decode(TransactionReferenceCodec::encode(transaction)?.encode_to_vec())?, MessageContent::TransactionReference(value) if value.network_id == "1")
    );
    let calls = WalletSendCalls {
        version: "1".into(),
        chain_id: "1".into(),
        from: "0x1".into(),
        calls: vec![],
        capabilities: None,
    };
    assert!(
        matches!(MessageContent::decode(WalletSendCallsCodec::encode(calls)?.encode_to_vec())?, MessageContent::WalletSendCalls(value) if value.chain_id == "1")
    );
    let intent = Intent {
        id: "intent".into(),
        action_id: "action".into(),
        metadata: None,
    };
    assert!(
        matches!(MessageContent::decode(IntentCodec::encode(intent)?.encode_to_vec())?, MessageContent::Intent(value) if value.action_id == "action")
    );
    let actions = Actions {
        id: "actions".into(),
        description: "desc".into(),
        actions: vec![Action {
            id: "one".into(),
            label: "One".into(),
            image_url: None,
            style: None,
            expires_at: None,
        }],
        expires_at: None,
    };
    assert!(
        matches!(MessageContent::decode(ActionsCodec::encode(actions)?.encode_to_vec())?, MessageContent::Actions(value) if value.id == "actions")
    );
    let update = GroupUpdated {
        initiated_by_inbox_id: "inbox".into(),
        ..Default::default()
    };
    assert!(
        matches!(MessageContent::decode(GroupUpdatedCodec::encode(update)?.encode_to_vec())?, MessageContent::GroupUpdated(value) if value.initiated_by_inbox_id.0 == "inbox")
    );
    let leave = LeaveRequest {
        authenticated_note: Some(b"note".to_vec()),
    };
    assert!(
        matches!(MessageContent::decode(LeaveRequestCodec::encode(leave)?.encode_to_vec())?, MessageContent::LeaveRequest(value) if value.authenticated_note == Some(b"note".to_vec()))
    );
}
