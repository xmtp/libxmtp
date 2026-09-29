use super::*;
use xmtp_content_types::ContentCodec;

// verifies: CTYPE-010
#[xmtp_common::test(unwrap_try = true)]
fn catalogue_push_defaults_match_standard_codecs() {
    use xmtp_content_types::{
        actions::ActionsCodec, attachment::AttachmentCodec, delete_message::DeleteMessageCodec,
        group_updated::GroupUpdatedCodec, intent::IntentCodec, leave_request::LeaveRequestCodec,
        markdown::MarkdownCodec, membership_change::GroupMembershipChangeCodec,
        multi_remote_attachment::MultiRemoteAttachmentCodec, reaction::ReactionCodec,
        read_receipt::ReadReceiptCodec, remote_attachment::RemoteAttachmentCodec,
        reply::ReplyCodec, text::TextCodec, transaction_reference::TransactionReferenceCodec,
        wallet_send_calls::WalletSendCallsCodec,
    };

    macro_rules! check_codec {
        ($codec:ty) => {{
            let codec_type = <$codec>::content_type();
            let content_type = ContentTypeId {
                authority_id: codec_type.authority_id,
                type_id: codec_type.type_id,
                version_major: codec_type.version_major,
                version_minor: codec_type.version_minor,
            };
            assert_eq!(
                catalogue_push_default(&content_type),
                <$codec>::should_push(),
                "push default differs for {:?}",
                content_type
            );
        }};
    }
    check_codec!(TextCodec);
    check_codec!(MarkdownCodec);
    check_codec!(ReadReceiptCodec);
    check_codec!(ReactionCodec);
    check_codec!(AttachmentCodec);
    check_codec!(RemoteAttachmentCodec);
    check_codec!(MultiRemoteAttachmentCodec);
    check_codec!(TransactionReferenceCodec);
    check_codec!(WalletSendCallsCodec);
    check_codec!(ActionsCodec);
    check_codec!(IntentCodec);
    check_codec!(ReplyCodec);
    check_codec!(GroupUpdatedCodec);
    check_codec!(DeleteMessageCodec);
    check_codec!(LeaveRequestCodec);
    check_codec!(GroupMembershipChangeCodec);
    assert!(!catalogue_push_default(&ContentTypeId {
        authority_id: "xmtp.org".into(),
        type_id: "editMessage".into(),
        version_major: 1,
        version_minor: 0,
    }));
}
