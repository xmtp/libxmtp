//! Randomized backend subscription and transport delivery tests.
//! The server test checks every update acknowledgement, per-topic ordering,
//! and fixed target completion. The transport test checks delivery across
//! lease changes, slow consumers, and connection failures.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
use xmtp_mls::prelude::*;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{RngExt, SeedableRng};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;

use xmtp_api_backend::{
    BackendBinding, BidiConnection, BidiEvent, BidiTransport, DEFAULT_LEASE_DEPTH, LeaseEvent,
    OpenError, TopicLease, TransportBinding,
};
use xmtp_mls::tester;
use xmtp_mls::utils::{LocalTesterBuilder, TesterBuilder};
use xmtp_mls_engine::groups::MlsGroup;
use xmtp_mls_engine::groups::context::XmtpSharedContext;
use xmtp_proto::backend_v1::subscribe_request::Update;
use xmtp_proto::types::ReceivedEnvelope;
use xmtp_proto::types::Topic;

mod server_contract;
mod transport_delivery;

/// Wall-clock guard for the settle phases (catch-up completion, sentinel
/// delivery). Generous against a local node; hitting it IS the failure.
const SETTLE: Duration = Duration::from_secs(30);

/// Random publish bursts stop once a group holds this many messages to bound
/// the test workload. Racing producers reserve
/// a slot (`fetch_add`) before each send, so the cap holds under
/// contention. Sentinels are exempt.
const PUBLISH_CAP: usize = 60;

fn fuzz_seed() -> u64 {
    match std::env::var("XMTP_BIDI_FUZZ_SEED") {
        Ok(seed) => seed.parse().expect("XMTP_BIDI_FUZZ_SEED must be a u64"),
        Err(_) => rand::rng().random(),
    }
}

fn fuzz_rounds(default: usize) -> usize {
    match std::env::var("XMTP_BIDI_FUZZ_ROUNDS") {
        Ok(rounds) => rounds
            .parse()
            .expect("XMTP_BIDI_FUZZ_ROUNDS must be a usize"),
        Err(_) => default,
    }
}

/// Keep the reproduction seed in the failure summary even when CI trims logs.
async fn publish_message<C: XmtpSharedContext>(group: &MlsGroup<C>, payload: &[u8], seed: u64) {
    group
        .send_message(payload, Default::default())
        .await
        .unwrap_or_else(|error| panic!("fuzz publish failed (seed={seed}): {error:?}"));
}

/// Join every group from its welcome. One `sync_welcomes` can miss a welcome
/// sent just before it, because its target comes from a read that can lag the
/// primary (see `wait_for_welcomes`). Poll until all of the groups arrive.
async fn joined_groups(
    producer: &xmtp_mls::utils::ClientTester,
    group_ids: &[xmtp_proto::types::GroupId],
) -> Vec<xmtp_mls::utils::TestMlsGroup> {
    xmtp_common::wait_for_some(|| async {
        producer
            .sync_welcomes()
            .await
            .expect("producer welcome sync failed");
        group_ids.iter().map(|id| producer.group(id).ok()).collect()
    })
    .await
    .expect("producer never received every group welcome")
}

fn gm_parts(message: &ReceivedEnvelope) -> Option<(Topic, u64)> {
    let meta = message.meta()?;
    Some((
        Topic::parse(&meta.topic.as_ref()?.topic).ok()?,
        meta.cursor.as_ref()?.sequence_id,
    ))
}

/// The server's authoritative per-topic message ids, queried over the
/// regular unary API — the same source of truth the durable path syncs from.
/// `query_group_messages` reads all pages through `query_all`, which follows
/// `has_more` and rejects continuation without cursor progress. No single-page
/// size guard is needed here.
async fn ground_truth<C>(
    api: &xmtp_api::ApiClientWrapper<C>,
    groups: &[(xmtp_proto::types::GroupId, Topic)],
) -> HashMap<Topic, BTreeSet<u64>>
where
    C: xmtp_proto::api_client::XmtpBackendClient,
{
    let mut truth: HashMap<Topic, BTreeSet<u64>> = HashMap::new();
    for (group_id, topic) in groups {
        let messages = api
            .query_group_messages(*group_id)
            .await
            .unwrap_or_else(|_| panic!("ground-truth query failed"));
        let ids = truth.entry(topic.clone()).or_default();
        for message in &messages {
            ids.insert(message.cursor.0);
        }
    }
    truth
}
