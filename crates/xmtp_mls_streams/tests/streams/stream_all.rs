use xmtp_db::{
    consent_record::ConsentState, group::ConversationType, group_message::StoredGroupMessage,
};
use xmtp_mls::subscriptions::{
    SubscribeError,
    local_delivery::{DeliveryScope, LocalDeliveryFilter},
    stream_all_messages,
};
use xmtp_mls_engine::groups::context::XmtpSharedContext;
use xmtp_mls_engine::groups::send_message_opts::SendMessageOpts;

use futures::StreamExt;
use rstest::*;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs};
use xmtp_id::associations::test_utils::WalletTestExt;
use xmtp_mls::builder::ClientBuilder;
use xmtp_mls::subscriptions::local_delivery::LocalDelivery;
use xmtp_mls::subscriptions::stream_messages::stream_stats::StreamWithStats;
use xmtp_mls::tester;
use xmtp_proto::types::GroupId;

mod concurrency;
mod consent_and_cursors;
mod group_list;

async fn assert_retained_history<C, S>(context: &C, stream: &mut S, filter: LocalDeliveryFilter)
where
    C: XmtpSharedContext + 'static,
    S: futures::Stream<Item = Result<StoredGroupMessage>> + Unpin,
{
    let snapshot =
        LocalDelivery::history_snapshot(context, &DeliveryScope::All, &filter, 100).unwrap();
    assert!(!snapshot.messages.is_empty());
    for retained in snapshot.messages {
        assert_eq!(stream.next().await.unwrap().unwrap(), retained.message);
    }
}

async fn assert_retained_join<C, S>(context: &C, stream: &mut S, group_id: GroupId)
where
    C: XmtpSharedContext + 'static,
    S: futures::Stream<Item = Result<StoredGroupMessage>> + Unpin,
{
    let message = stream.next().await.unwrap().unwrap();
    assert_eq!(message.group_id, group_id);
    assert_eq!(message.kind, GroupMessageKind::MembershipChange);
    let snapshot = LocalDelivery::history_snapshot(
        context,
        &DeliveryScope::Groups(vec![group_id]),
        &LocalDeliveryFilter::default(),
        100,
    )
    .unwrap();
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(snapshot.messages[0].message, message);
}

type Result<T> = std::result::Result<T, SubscribeError>;

// ---------------------------------------------------------------------------
// Stream-liveness watchdog (see `subscriptions::watchdog`).
// ---------------------------------------------------------------------------
