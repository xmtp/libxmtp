use crate::groups::GroupError;
use crate::groups::error::DeleteMessageError;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::test_support::message_rows::cached_text_row;
use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestPeer, EngineTestPeerOps};
use xmtp_content_types::{ContentCodec, compression::compress, text::TextCodec};
use xmtp_db::Store;
use xmtp_db::group_message::{
    ContentType, DeliveryStatus, GroupMessageKind, MsgQueryArgs, QueryGroupMessage,
};
use xmtp_db::message_deletion::QueryMessageDeletion;
use xmtp_mls_messages::decoded_message::{DeletedBy, MessageBody};
use xmtp_proto::xmtp::mls::message_contents::Compression;

mod applied_deletions;
mod deleted_message_views;
mod rejected_deletions;

/// The inbox that marked `id` deleted, or `None` while it is unmarked.
fn mark_of(db: &impl QueryGroupMessage, id: &[u8]) -> Option<String> {
    db.get_group_message(id)
        .expect("message read")
        .and_then(|message| message.deleted_by_inbox_id)
}
