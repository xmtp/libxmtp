//! A commit-log read returns every envelope it can place on its topic, so one
//! bad envelope never fails the read.
use super::*;
use wire::client_envelope::Payload;
use xmtp_mls_common::{
    commit_log::seal_entry,
    group_keys::{GroupKeys, GroupRoot},
};
use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

fn row(topic: &Topic, sequence: u64, payload: Option<Payload>) -> wire::ServerEnvelope {
    wire::ServerEnvelope {
        meta: Some(wire::EnvelopeMeta {
            topic: Some(wire::Topic {
                topic: topic.cloned_vec(),
            }),
            cursor: Some(wire::Cursor {
                sequence_id: sequence,
            }),
            server_ns: 123,
            message_hash: Some(wire::MessageHash {
                hash: Some(wire::message_hash::Hash::Sha256(vec![7; 32])),
            }),
            ..Default::default()
        }),
        envelope: Some(wire::ClientEnvelope { payload }),
    }
}

/// An envelope that holds no sealed entry comes back without one, so the
/// reader discards it and still reads past it. An envelope whose metadata
/// does not decode is left out. Neither fails the read, and the order holds.
// verifies: FORK-075
#[xmtp_common::test(unwrap_try = true)]
async fn one_bad_commit_log_envelope_does_not_fail_the_read() {
    let keys = GroupKeys::derive(&GroupRoot::from_bytes(&[3; 32])?);
    let topic = Topic::new_commit_log(keys.commit_log_id());
    let sealed = |commit_sequence_id| {
        let entry = PlaintextCommitLogEntry {
            group_id: keys.group_id().to_vec(),
            commit_sequence_id,
            ..Default::default()
        };
        seal_entry(&keys, &entry).unwrap()
    };
    let (first, last) = (sealed(1), sealed(5));
    let mut no_hash = row(&topic, 4, Some(Payload::SealedCommitLogEntry(sealed(4))));
    no_hash.meta.as_mut()?.message_hash = None;
    let envelopes = vec![
        row(
            &topic,
            1,
            Some(Payload::SealedCommitLogEntry(first.clone())),
        ),
        row(
            &topic,
            2,
            Some(Payload::CommitLogEntry(wire::CommitLogEntry::default())),
        ),
        row(&topic, 3, None),
        no_hash,
        row(&topic, 5, Some(Payload::SealedCommitLogEntry(last.clone()))),
    ];
    let mut mock = MockBackendClient::new();
    mock.expect_query().times(1).returning(move |_| {
        Ok(wire::QueryResponse {
            envelopes: envelopes.clone(),
            continuation: Some(wire::Continuation { has_more: false }),
        })
    });
    let logs = wrapper(mock)
        .query_commit_log([(topic.clone(), Cursor(0))].into())
        .await?;
    let read: Vec<_> = logs
        .iter()
        .map(|log| (log.cursor.0, log.sealed.clone()))
        .collect();
    assert_eq!(
        read,
        vec![(1, Some(first)), (2, None), (3, None), (5, Some(last))]
    );
    assert!(logs.iter().all(|log| log.topic == topic));
}
