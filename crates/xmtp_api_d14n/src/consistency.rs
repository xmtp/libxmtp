//! D14n network consistency checker.
//!
//! Provides the [`NetworkConsistencyProvider`] trait and the
//! [`D14nConsistencyChecker`] implementation that polls every D14N replication
//! node via `QueryEnvelopes` until all required topics are visible at the
//! required originator cursors.

use crate::d14n::QueryEnvelopes;
use futures::StreamExt;
use prost::Message;
use std::time::Duration;
use xmtp_api_grpc::{ClientBuilder, GrpcClient};
use xmtp_common::RetryableError;
use xmtp_proto::api::{Client, Query};
use xmtp_proto::types::{Topic, TopicCursor};
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::EnvelopesQuery;

use crate::middleware::multi_node_client::gateway_api::get_nodes;

// ──────────────────────────────────────────────────────────────────────────────
// Trait definitions (shared types used by both this crate and xmtp_api)
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum NetworkConsistencyQuorum {
    AllNodes,
    Majority,
    Count(u32),
}

impl NetworkConsistencyQuorum {
    pub fn required(&self, total: usize) -> usize {
        match self {
            Self::AllNodes => total,
            Self::Majority => total / 2 + 1,
            Self::Count(n) => (*n as usize).min(total),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NetworkConsistencyOpts {
    pub quorum: NetworkConsistencyQuorum,
    /// Maximum number of poll attempts per node before giving up.
    pub max_attempts: u32,
    /// Starting backoff delay in milliseconds between retries.
    pub initial_delay_ms: u64,
    /// Maximum backoff delay in milliseconds (cap for exponential backoff).
    pub max_delay_ms: u64,
    /// Overall wall-clock timeout in milliseconds; whichever fires first
    /// (timeout_ms or max_attempts exhaustion) terminates the poll.
    pub timeout_ms: u64,
}

impl Default for NetworkConsistencyOpts {
    fn default() -> Self {
        Self {
            quorum: NetworkConsistencyQuorum::AllNodes,
            max_attempts: 10,
            initial_delay_ms: 100,
            max_delay_ms: 2_000,
            timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkConsistencyError {
    #[error("Quorum not reached: {confirmed}/{required} nodes confirmed within timeout")]
    QuorumNotReached { confirmed: usize, required: usize },
    #[error("Node discovery failed: {0}")]
    NodeDiscovery(String),
    #[error("Consistency check timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
}

impl RetryableError for NetworkConsistencyError {
    fn is_retryable(&self) -> bool {
        false
    }
}

#[xmtp_common::async_trait]
pub trait NetworkConsistencyProvider: Send + Sync {
    async fn wait_until_visible(
        &self,
        topics: xmtp_proto::types::TopicCursor,
        opts: &NetworkConsistencyOpts,
    ) -> Result<(), NetworkConsistencyError>;
}

// ──────────────────────────────────────────────────────────────────────────────
// Helper: cursor satisfaction logic
// ──────────────────────────────────────────────────────────────────────────────

/// Check whether all topics in `topics` are satisfied by the set of
/// `OriginatorEnvelope`s returned from a single node.
///
/// A topic is satisfied when, for each `(originator_id, required_seq)` entry
/// in the topic's `GlobalCursor`, there exists at least one envelope for that
/// topic whose decoded `originator_node_id == originator_id` and
/// `originator_sequence_id >= required_seq`.
///
/// If the `GlobalCursor` for a topic is empty (no entries), the topic is
/// vacuously satisfied — any envelope on that topic (or no envelope at all)
/// is sufficient.
fn all_topics_satisfied(
    topics: &TopicCursor,
    envelopes: &[xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope],
) -> bool {
    topics.iter().all(|(topic, global_cursor)| {
        let topic_bytes = topic.cloned_vec();

        // Decode all envelopes for this specific topic once, up-front.
        let topic_envelopes: Vec<(u32, u64)> = envelopes
            .iter()
            .filter_map(|env| {
                let unsigned =
                    UnsignedOriginatorEnvelope::decode(env.unsigned_originator_envelope.as_slice())
                        .ok()?;

                // Decode the payer envelope to reach the client envelope.
                let payer_env = xmtp_proto::xmtp::xmtpv4::envelopes::PayerEnvelope::decode(
                    unsigned.payer_envelope_bytes.as_slice(),
                )
                .ok()?;

                // Decode the client envelope to retrieve the target topic.
                let client_env = xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope::decode(
                    payer_env.unsigned_client_envelope.as_slice(),
                )
                .ok()?;

                if client_env.aad.as_ref().map(|a| &a.target_topic) != Some(&topic_bytes) {
                    return None;
                }

                Some((unsigned.originator_node_id, unsigned.originator_sequence_id))
            })
            .collect();

        // For each (originator_id, required_seq) in the GlobalCursor, there
        // must be at least one envelope from that originator with
        // sequence_id >= required_seq.
        //
        // If GlobalCursor is empty, cursors() yields nothing and all() returns
        // true vacuously — meaning an empty cursor is satisfied by anything.
        global_cursor.cursors().all(|required_cursor| {
            topic_envelopes.iter().any(|&(orig_id, seq_id)| {
                orig_id == required_cursor.originator_id && seq_id >= required_cursor.sequence_id
            })
        })
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Poll a single node until all topics are visible or max_attempts is reached.
///
/// `node_id` is used only for diagnostic tracing.
async fn poll_until_visible(
    node_client: GrpcClient,
    node_id: u32,
    topics: TopicCursor,
    opts: &NetworkConsistencyOpts,
) -> bool {
    let topic_bytes: Vec<Vec<u8>> = topics.keys().map(Topic::cloned_vec).collect();

    let mut delay_ms = opts.initial_delay_ms;

    for attempt in 0..opts.max_attempts {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            delay_ms = (delay_ms * 2).min(opts.max_delay_ms);
        }

        let mut endpoint = match QueryEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: topic_bytes.clone(),
                originator_node_ids: vec![],
                last_seen: None,
            })
            .build()
        {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(node_id, "Failed to build QueryEnvelopes: {}", e);
                continue;
            }
        };

        let response = match endpoint.query(&node_client).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(node_id, attempt, "QueryEnvelopes failed: {}", e);
                continue;
            }
        };

        if all_topics_satisfied(&topics, &response.envelopes) {
            tracing::debug!(node_id, attempt, "All topics satisfied");
            return true;
        }

        tracing::debug!(
            node_id,
            attempt,
            envelopes = response.envelopes.len(),
            "Topics not yet satisfied",
        );
    }

    false
}

// ──────────────────────────────────────────────────────────────────────────────
// D14nConsistencyChecker
// ──────────────────────────────────────────────────────────────────────────────

/// Implements [`NetworkConsistencyProvider`] by polling every D14N replication
/// node via `QueryEnvelopes` until all required topics are visible at the
/// required originator cursors.
pub struct D14nConsistencyChecker<C> {
    gateway_client: C,
    node_client_template: ClientBuilder,
}

impl<C> D14nConsistencyChecker<C> {
    pub fn new(gateway_client: C, node_client_template: ClientBuilder) -> Self {
        Self {
            gateway_client,
            node_client_template,
        }
    }
}

#[xmtp_common::async_trait]
impl<C> NetworkConsistencyProvider for D14nConsistencyChecker<C>
where
    C: Client + Send + Sync,
{
    async fn wait_until_visible(
        &self,
        topics: TopicCursor,
        opts: &NetworkConsistencyOpts,
    ) -> Result<(), NetworkConsistencyError> {
        let nodes = get_nodes(&self.gateway_client, &self.node_client_template)
            .await
            .map_err(|e| NetworkConsistencyError::NodeDiscovery(e.to_string()))?;

        let total = nodes.len();
        let required = opts.quorum.required(total);

        let topics_arc = std::sync::Arc::new(topics);
        let opts_arc = std::sync::Arc::new(opts.clone());

        // Use FuturesUnordered so we can yield results as each node finishes.
        // When quorum is reached we break out of the loop and drop the stream,
        // which cancels any in-flight futures. This is acceptable: we already
        // have the confirmation we need, and the cancelled polls have no
        // side effects.
        let mut stream: futures::stream::FuturesUnordered<_> =
            nodes
                .into_iter()
                .map(|(node_id, node_client)| {
                    let topics = topics_arc.clone();
                    let opts = opts_arc.clone();
                    async move {
                        poll_until_visible(node_client, node_id, (*topics).clone(), &opts).await
                    }
                })
                .collect();

        let timeout_duration = Duration::from_millis(opts.timeout_ms);

        let confirmed = tokio::time::timeout(timeout_duration, async move {
            let mut confirmed = 0usize;

            while let Some(success) = stream.next().await {
                if success {
                    confirmed += 1;
                    if confirmed >= required {
                        return confirmed;
                    }
                }
            }
            confirmed
        })
        .await
        .map_err(|_| NetworkConsistencyError::Timeout {
            timeout_ms: opts.timeout_ms,
        })?;

        if confirmed >= required {
            Ok(())
        } else {
            Err(NetworkConsistencyError::QuorumNotReached {
                confirmed,
                required,
            })
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_proto::types::{Cursor, GlobalCursor};

    #[xmtp_common::test]
    fn empty_cursor_satisfied_by_any_envelope() {
        let cursor = GlobalCursor::default();
        // An empty GlobalCursor has no originator requirements.
        // cursors().all(...) vacuously returns true.
        assert!(
            cursor.cursors().next().is_none(),
            "default GlobalCursor has no entries"
        );
        // Confirm that all_topics_satisfied with an empty GlobalCursor returns
        // true even when there are no envelopes (vacuously satisfied).
        let mut topics = TopicCursor::default();
        topics.insert(Topic::new_group_message(b"group1"), GlobalCursor::default());
        // No envelopes — empty cursor should still be satisfied vacuously.
        assert!(
            all_topics_satisfied(&topics, &[]),
            "empty GlobalCursor should be vacuously satisfied even with no envelopes"
        );
    }

    #[xmtp_common::test]
    fn global_cursor_tracks_multiple_originators() {
        // Build a GlobalCursor tracking two originators.
        let mut cursor = GlobalCursor::default();
        cursor.apply(&Cursor::new(10, 1u32)); // originator 1 is at seq 10
        cursor.apply(&Cursor::new(5, 2u32)); // originator 2 is at seq 5

        // All entries must be present.
        assert_eq!(cursor.cursors().count(), 2);

        // has_seen(other) means "does self have seq >= other.sequence_id for that originator?"
        // cursor has orig 1 at seq 10; it has seen seq 9 (10 >= 9), seq 10 (10 >= 10)
        // but NOT seq 11 (10 < 11).
        assert!(
            cursor.has_seen(&Cursor::new(9, 1u32)),
            "cursor at seq 10 has seen seq 9"
        );
        assert!(
            cursor.has_seen(&Cursor::new(10, 1u32)),
            "cursor at seq 10 has seen seq 10"
        );
        assert!(
            !cursor.has_seen(&Cursor::new(11, 1u32)),
            "cursor at seq 10 has NOT seen seq 11"
        );
        assert!(
            cursor.has_seen(&Cursor::new(5, 2u32)),
            "cursor at seq 5 has seen seq 5"
        );
        assert!(
            !cursor.has_seen(&Cursor::new(6, 2u32)),
            "cursor at seq 5 has NOT seen seq 6"
        );
    }

    #[xmtp_common::test]
    fn has_seen_unknown_originator_returns_false() {
        let mut cursor = GlobalCursor::default();
        cursor.apply(&Cursor::new(5, 1u32)); // originator 1 is at seq 5
        // Originator 2 is not tracked; get() returns 0, so has_seen with any seq > 0 is false.
        assert!(
            !cursor.has_seen(&Cursor::new(1, 2u32)),
            "unknown originator 2 - cursor returns 0, so seq 1 is not seen"
        );
    }
}
