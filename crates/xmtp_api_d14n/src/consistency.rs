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
use xmtp_proto::types::{GlobalCursor, Topic, TopicCursor};
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

/// Returns true if an envelope from `originator_node_id` with `sequence_id`
/// satisfies the given `GlobalCursor` constraint.
///
/// - If the cursor is empty (no originator entries), any envelope satisfies it.
/// - Otherwise, the envelope must come from an originator tracked by the cursor
///   and have `sequence_id >= required`.
pub(crate) fn cursor_satisfied(
    cursor: &GlobalCursor,
    originator_node_id: u32,
    sequence_id: u64,
) -> bool {
    if cursor.is_empty() {
        return true;
    }
    // `GlobalCursor::get` returns 0 for unknown originators.
    let required_seq = cursor.get(&originator_node_id);
    if required_seq == 0 {
        // Originator not tracked in this cursor - does not satisfy.
        return false;
    }
    sequence_id >= required_seq
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Check whether all topics in `topics` are satisfied by the set of
/// `OriginatorEnvelope`s returned from a single node.
fn all_topics_satisfied(
    topics: &TopicCursor,
    envelopes: &[xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope],
) -> bool {
    for (topic, global_cursor) in topics {
        let topic_bytes = topic.cloned_vec();

        let satisfied = envelopes.iter().any(|env| {
            // Decode the unsigned originator envelope.
            let Ok(unsigned) =
                UnsignedOriginatorEnvelope::decode(env.unsigned_originator_envelope.as_slice())
            else {
                return false;
            };

            // Decode the payer envelope to reach the client envelope.
            let payer_env = match xmtp_proto::xmtp::xmtpv4::envelopes::PayerEnvelope::decode(
                unsigned.payer_envelope_bytes.as_slice(),
            ) {
                Ok(p) => p,
                Err(_) => return false,
            };

            // Decode the client envelope to retrieve the target topic.
            let client_env =
                match xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope::decode(
                    payer_env.unsigned_client_envelope.as_slice(),
                ) {
                    Ok(c) => c,
                    Err(_) => return false,
                };

            if client_env.aad.as_ref().map(|a| &a.target_topic) != Some(&topic_bytes) {
                return false;
            }

            cursor_satisfied(
                global_cursor,
                unsigned.originator_node_id,
                unsigned.originator_sequence_id,
            )
        });

        if !satisfied {
            return false;
        }
    }
    true
}

/// Poll a single node until all topics are visible or max_attempts is reached.
async fn poll_until_visible(
    node_client: GrpcClient,
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
                tracing::warn!("Failed to build QueryEnvelopes: {}", e);
                continue;
            }
        };

        let response = match endpoint.query(&node_client).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("QueryEnvelopes failed on attempt {}: {}", attempt, e);
                continue;
            }
        };

        if all_topics_satisfied(&topics, &response.envelopes) {
            return true;
        }

        tracing::debug!(
            "Attempt {}: topics not yet satisfied ({} envelopes returned)",
            attempt,
            response.envelopes.len()
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

        let tasks: Vec<_> = nodes
            .into_values()
            .map(|node_client| {
                let topics = topics_arc.clone();
                let opts = opts_arc.clone();
                async move { poll_until_visible(node_client, (*topics).clone(), &opts).await }
            })
            .collect();

        let timeout_duration = Duration::from_millis(opts.timeout_ms);

        let confirmed = tokio::time::timeout(timeout_duration, async move {
            let mut confirmed = 0usize;
            let mut stream = futures::stream::iter(tasks).buffer_unordered(total.max(1));

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
            Err(NetworkConsistencyError::QuorumNotReached { confirmed, required })
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

    // Test cursor_satisfied helper function
    #[xmtp_common::test]
    fn empty_cursor_satisfied_by_any_envelope() {
        let cursor = GlobalCursor::default();
        // An empty GlobalCursor should be satisfied by any originator/sequence
        let satisfied = cursor_satisfied(&cursor, 1, 999);
        assert!(satisfied, "empty cursor should accept any originator/sequence");
    }

    #[xmtp_common::test]
    fn non_empty_cursor_requires_min_sequence() {
        let mut cursor = GlobalCursor::default();
        cursor.apply(&Cursor::new(10, 1u32)); // node 1 must have seq >= 10
        assert!(
            !cursor_satisfied(&cursor, 1, 9),
            "seq 9 should NOT satisfy min=10"
        );
        assert!(
            cursor_satisfied(&cursor, 1, 10),
            "seq 10 should satisfy min=10"
        );
        assert!(
            cursor_satisfied(&cursor, 1, 11),
            "seq 11 should satisfy min=10"
        );
    }

    #[xmtp_common::test]
    fn cursor_wrong_originator_not_satisfied() {
        let mut cursor = GlobalCursor::default();
        cursor.apply(&Cursor::new(5, 1u32)); // node 1 must have seq >= 5
        // Envelope from originator 2, not 1 - should not satisfy
        assert!(
            !cursor_satisfied(&cursor, 2, 100),
            "wrong originator should not satisfy"
        );
    }
}
