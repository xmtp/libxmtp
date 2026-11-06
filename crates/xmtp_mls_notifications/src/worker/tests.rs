use super::*;
use crate::client::{NotificationOverride, NotificationState, encode_config};
use std::sync::Arc;
use xmtp_common::{
    RetryableError, StreamHandle,
    time::{Duration, timeout},
    wait_for_eq,
};
use xmtp_db::{consent_record::ConsentState, group::GroupMembershipState};

pub(crate) mod support {
    pub(crate) use crate::client::test_support::*;
    pub(crate) use crate::worker::test_hooks::on_before_request;
}
pub(crate) use crate::client::test_support::config;
use support::{Call, NotificationPeer};
use xmtp_mls_core::context::CoreContext;
use xmtp_mls_engine::groups::test_support::{
    EnginePeer, EngineTestPeer, EngineTestPeerOps, MlsGroupExt,
};

mod desired_set;
mod enable_disable;
mod failures_and_repair;
mod rate_limit_holds;

fn synthetic_desired(count: u128) -> Desired {
    (1..=count)
        .map(|index| {
            let id = GroupId::try_from(index.to_le_bytes().as_slice()).unwrap();
            (
                Topic::new_group_message(id).cloned_vec(),
                DesiredTopic {
                    group_id: Some(id),
                    include_commits: false,
                },
            )
        })
        .collect()
}
