use crate::groups::MlsGroup;
use crate::groups::PolicySet;
use crate::groups::commit_log::{
    CommitLogTestFunction, CommitLogWorker, Factory, should_skip_remote_commit_log_entry,
};
use crate::groups::context::XmtpSharedContext;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestPeer, EngineTestPeerOps};
use crate::prelude::*;
use openmls::prelude::{OpenMlsCrypto, SignatureScheme};
use openmls_rust_crypto::RustCrypto;
use prost::Message;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use xmtp_common::time::Duration;
use xmtp_db::consent_record::{ConsentState, ConsentType, StoredConsentRecord};
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group::{ConversationType, GroupMembershipState};
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::local_commit_log::{CommitType, LocalCommitLog, NewLocalCommitLog};
use xmtp_db::prelude::*;
use xmtp_db::refresh_state::EntityKind;
use xmtp_db::remote_commit_log::{CommitResult, RemoteCommitLog, RemoteCommitLogOrder};
use xmtp_mls_group_keys::commit_log::{open_entry, seal_entry, sign_entry_author};
use xmtp_mls_group_keys::group_keys::{GroupKeys, GroupRoot, SealError};
use xmtp_mls_group_state::app_data::group_root::{group_keys_from_extensions, read_group_root};
use xmtp_mls_group_state::group::GroupMetadataOptions;
use xmtp_proto::backend_v1::{self as wire, SealedCommitLogEntry, client_envelope::Payload};
use xmtp_proto::types::Cursor;
use xmtp_proto::types::GroupId;
use xmtp_proto::types::Topic;
use xmtp_proto::xmtp::mls::message_contents::CommitResult as ProtoCommitResult;
use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

mod scripted;

mod download;
mod entry_verification;
mod publish;
mod publisher_role;

/// The keys of a conversation's group root, as its commit-log worker derives
/// them.
fn keys_of(group: &MlsGroup<impl XmtpSharedContext>) -> GroupKeys {
    group_keys_from_extensions(group.load_group_context().unwrap().extensions()).unwrap()
}

fn server_envelope(topic: &Topic, sequence_id: u64, payload: Payload) -> wire::ServerEnvelope {
    wire::ServerEnvelope {
        meta: Some(wire::EnvelopeMeta {
            topic: Some(wire::Topic {
                topic: topic.cloned_vec(),
            }),
            cursor: Some(wire::Cursor { sequence_id }),
            server_ns: 1,
            message_hash: Some(wire::MessageHash {
                hash: Some(wire::message_hash::Hash::Sha256(vec![
                    sequence_id as u8;
                    32
                ])),
            }),
            ..Default::default()
        }),
        envelope: Some(wire::ClientEnvelope {
            payload: Some(payload),
        }),
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Read a conversation's commit-log topic from the backend and open every
/// entry with its keys.
async fn remote_entries(
    client: &EngineTestPeer,
    group: &MlsGroup<impl XmtpSharedContext>,
) -> Vec<PlaintextCommitLogEntry> {
    let keys = keys_of(group);
    client
        .context
        .api()
        .query_commit_log([(Topic::new_commit_log(keys.commit_log_id()), Cursor(0))].into())
        .await
        .unwrap()
        .iter()
        .map(|read| {
            open_entry(
                &keys,
                group.group_id.as_slice(),
                read.cursor.0,
                read.sealed.as_ref().unwrap(),
            )
            .unwrap()
        })
        .collect()
}

/// The conversations one publish turn advanced.
async fn publish_turn<C: XmtpSharedContext + 'static>(
    worker: &mut CommitLogWorker<C>,
) -> Vec<Vec<u8>> {
    worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
        .await
        .unwrap()[0]
        .publish_commit_log_results
        .as_ref()
        .unwrap()
        .iter()
        .map(|published| published.conversation_id.clone())
        .collect()
}
