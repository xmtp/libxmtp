//! Native-only HTTP mock for testing streams
use xmtp_proto::mls_v1::{
    group_message::{self, V1},
    GroupMessage,
};

/// Generate n random Group Messages messages
pub fn generate_messages(n: usize) -> Vec<GroupMessage> {
    let mut msgs = vec![];
    for _ in 0..n {
        let m = GroupMessage {
            version: Some(group_message::Version::V1(V1 {
                id: xmtp_common::rand_u64(),
                created_ns: xmtp_common::rand_u64(),
                group_id: xmtp_common::rand_vec::<16>(),
                data: xmtp_common::rand_vec::<256>(),
                sender_hmac: vec![],
                should_push: false,
            })),
        };
        msgs.push(m);
    }
    msgs
}
