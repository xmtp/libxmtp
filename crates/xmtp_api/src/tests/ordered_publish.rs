//! Publish requests carry at most `MAX_PUBLISH_ENVELOPES` envelopes (API
//! section 7), and commit-log chunks go out one at a time, in order.
use super::*;
use std::sync::Mutex;
use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

const GROUP: [u8; 16] = [9; 16];

fn group_unit(kinds: &[GroupMessageKind]) -> PublishUnit {
    PublishUnit::new(
        kinds
            .iter()
            .map(|kind| group_message_envelope(GROUP, *kind, []))
            .collect(),
    )
    .unwrap()
}

fn chunk_lengths(units: &[PublishUnit]) -> crate::Result<Vec<usize>> {
    Ok(chunk_publish(units)?
        .iter()
        .map(|chunk| crate::chunk::request(chunk).envelopes.len())
        .collect())
}

/// 2500 single-envelope units on one topic go out as requests
/// of at most 1000 envelopes. Bytes and topics stay far under their bounds,
/// so only the envelope bound splits them.
// verifies: CONF-073
#[xmtp_common::test(unwrap_try = true)]
fn publish_chunks_carry_at_most_the_envelope_bound() {
    let single = group_unit(&[GroupMessageKind::Application]);
    let units = vec![single.clone(); 2500];
    assert_eq!(chunk_lengths(&units)?, vec![1000, 1000, 500]);
    assert_eq!(MAX_PUBLISH_ENVELOPES, 1000);
}

/// A 3-envelope unit (two proposals and a commit) is never
/// split across requests, even when that leaves a request short of the bound.
// verifies: CONF-073
#[xmtp_common::test(unwrap_try = true)]
fn publish_chunks_keep_a_unit_whole_at_the_envelope_bound() {
    use GroupMessageKind::*;
    let single = group_unit(&[Application]);
    let commit = group_unit(&[Proposal, Proposal, Commit]);
    let mut units = vec![single.clone(); 999];
    units.push(commit);
    units.extend(vec![single; 997]);
    assert_eq!(chunk_lengths(&units)?, vec![999, 1000]);
    let chunks = chunk_publish(&units)?;
    assert_eq!(crate::chunk::request(&chunks[1][..1]).envelopes.len(), 3);
}

/// A unit of more than 1000 envelopes is refused before any
/// request, with the error the client treats as a definite failure.
// verifies: CONF-073
#[xmtp_common::test(unwrap_try = true)]
fn publish_unit_over_the_envelope_bound_is_refused_locally() {
    let envelope = group_message_envelope(GROUP, GroupMessageKind::Application, []);
    PublishUnit::new(vec![envelope.clone(); MAX_PUBLISH_ENVELOPES])?;
    let error = PublishUnit::new(vec![envelope; MAX_PUBLISH_ENVELOPES + 1]).unwrap_err();
    assert!(matches!(error, ApiError::UnitTooLarge), "{error:?}");
    assert!(!error.is_retryable());
}

fn commit_log_entries(count: u64) -> Vec<wire::CommitLogEntry> {
    (1..=count)
        .map(|sequence| {
            let Some(wire::client_envelope::Payload::CommitLogEntry(entry)) =
                commit_log_envelope_at(GROUP, sequence).payload
            else {
                unreachable!()
            };
            entry
        })
        .collect()
}

fn commit_sequence_ids(request: &wire::PublishRequest) -> Vec<u64> {
    request
        .envelopes
        .iter()
        .map(|envelope| {
            let Some(wire::client_envelope::Payload::CommitLogEntry(entry)) = &envelope.payload
            else {
                unreachable!()
            };
            PlaintextCommitLogEntry::decode(entry.serialized_commit_log_entry.as_slice())
                .unwrap()
                .commit_sequence_id
        })
        .collect()
}

/// A wrapper whose retries wait, so a request in backoff lets any other
/// in-flight request reach the mock first.
fn waiting_wrapper(mock: MockBackendClient) -> ApiClientWrapper<MockBackendClient> {
    let strategy = ExponentialBackoff::builder()
        .duration(Duration::from_millis(20))
        .max_jitter(Duration::ZERO)
        .build();
    ApiClientWrapper::new(
        mock,
        Retry::builder().with_strategy(strategy).retries(2).build(),
    )
}

/// 2500 entries of one conversation go out as three
/// requests in ascending `commit_sequence_id` order, each sent only after the
/// previous one returned. The first request fails once and waits to retry;
/// a concurrent sender would send the later chunks during that wait.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn commit_log_chunks_are_sent_one_at_a_time_in_order() {
    let calls = Arc::new(Mutex::new(Vec::<Vec<u64>>::new()));
    let seen = calls.clone();
    let mut mock = MockBackendClient::new();
    mock.expect_publish().times(4).returning(move |request| {
        let mut seen = seen.lock().unwrap();
        seen.push(commit_sequence_ids(&request));
        if seen.len() == 1 {
            return Err(status(tonic::Code::Unavailable));
        }
        Ok(published(request))
    });
    let mut stored = Vec::new();
    waiting_wrapper(mock)
        .publish_commit_log(commit_log_entries(2500), &mut stored)
        .await?;
    let calls = calls.lock().unwrap().clone();
    let first: Vec<u64> = (1..=1000).collect();
    assert_eq!(
        calls,
        vec![
            first.clone(),
            first,
            (1001..=2000).collect(),
            (2001..=2500).collect(),
        ]
    );
    assert_eq!(stored.len(), 2500);
}

/// Publishing stops at the first refused request, and `stored`
/// names exactly the entries stored before it, so a retry starts from the
/// first unstored entry.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn commit_log_publish_stops_at_the_first_failed_chunk() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mut mock = MockBackendClient::new();
    mock.expect_publish().times(2).returning(move |request| {
        if count.fetch_add(1, Ordering::SeqCst) == 1 {
            return Err(status(tonic::Code::InvalidArgument));
        }
        Ok(published(request))
    });
    let entries = commit_log_entries(2500);
    let mut stored = Vec::new();
    let result = wrapper(mock)
        .publish_commit_log(entries.clone(), &mut stored)
        .await;
    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let expected: Vec<_> = entries[..1000]
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            meta(
                &wire::ClientEnvelope {
                    payload: Some(wire::client_envelope::Payload::CommitLogEntry(
                        entry.clone(),
                    )),
                },
                i as u64 + 1,
            )
        })
        .collect();
    assert_eq!(stored, expected);
}
