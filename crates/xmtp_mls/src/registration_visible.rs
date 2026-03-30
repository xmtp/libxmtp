use xmtp_api_d14n::d14n::QueryEnvelopes;
use xmtp_api_d14n::protocol::traits::Envelope;
use xmtp_proto::api::{Client, Query};
use xmtp_proto::types::{Cursor, TopicKind};
use xmtp_proto::xmtp::xmtpv4::message_api::EnvelopesQuery;

use crate::client::ClientError;

/// Specifies how many nodes must confirm visibility before returning Ok(()).
#[derive(Debug, Clone)]
pub enum Quorum {
    /// Fraction of nodes that must confirm: required = ceil(p * node_count).
    Percentage(f32),
    /// Exact number of nodes that must confirm.
    Absolute(usize),
}

impl Quorum {
    pub fn required_count(&self, total: usize) -> usize {
        match self {
            Quorum::Percentage(p) => ((total as f32) * p).ceil() as usize,
            Quorum::Absolute(n) => *n,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VisibilityConfirmationOptions {
    pub quorum: Quorum,
    pub timeout_ms: u64,
    pub sleep_interval_ms: u64,
}

impl Default for VisibilityConfirmationOptions {
    fn default() -> Self {
        Self {
            quorum: Quorum::Percentage(0.5),
            timeout_ms: 30_000,
            sleep_interval_ms: 500,
        }
    }
}

/// Poll a single node until both the identity-update envelope and the
/// key-package envelope for this registration are visible, or until the
/// timeout elapses.
#[allow(dead_code)]
pub(crate) async fn check_node_visibility<C: Client>(
    node_client: &C,
    inbox_id: &str,
    installation_id: &[u8],
    cursor: Cursor,
    options: &VisibilityConfirmationOptions,
) -> Result<(), ClientError> {
    use xmtp_common::time::{Duration, Instant, sleep};

    let timeout = Duration::from_millis(options.timeout_ms);
    let sleep_interval = Duration::from_millis(options.sleep_interval_ms);

    // Decode the inbox_id hex string to bytes for the topic
    let inbox_id_bytes = hex::decode(inbox_id).unwrap_or_else(|_| inbox_id.as_bytes().to_vec());
    let identity_topic = TopicKind::IdentityUpdatesV1.create(&inbox_id_bytes);
    let key_package_topic = TopicKind::KeyPackagesV1.create(installation_id);

    let topics = vec![identity_topic.cloned_vec(), key_package_topic.cloned_vec()];

    let start = Instant::now();

    loop {
        let mut endpoint = QueryEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: topics.clone(),
                originator_node_ids: vec![],
                last_seen: None,
            })
            .build()
            .map_err(|e| {
                ClientError::Generic(format!("failed to build QueryEnvelopes endpoint: {e}"))
            })?;

        match endpoint.query(node_client).await {
            Err(e) => {
                tracing::warn!(
                    originator_id = cursor.originator_id,
                    error = %e,
                    "check_node_visibility: API error querying node, will retry"
                );
            }
            Ok(response) => {
                let mut identity_visible = false;
                let mut key_package_visible = false;

                for env in &response.envelopes {
                    let topic = match env.topic() {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!(
                                "check_node_visibility: failed to extract topic from envelope: {}",
                                e
                            );
                            continue;
                        }
                    };

                    match topic.kind() {
                        TopicKind::IdentityUpdatesV1 => {
                            if let Ok(env_cursor) = env.cursor()
                                && env_cursor.originator_id == cursor.originator_id
                                && env_cursor.sequence_id == cursor.sequence_id
                            {
                                identity_visible = true;
                            }
                        }
                        TopicKind::KeyPackagesV1 => {
                            key_package_visible = true;
                        }
                        _ => {}
                    }
                }

                if identity_visible && key_package_visible {
                    return Ok(());
                }
            }
        }

        if start.elapsed() >= timeout {
            return Err(ClientError::RegistrationNotVisible {
                failed_nodes: vec![cursor.originator_id],
            });
        }

        sleep(sleep_interval).await;
    }
}

#[cfg(test)]
mod quorum_tests {
    use super::*;

    #[test]
    fn quorum_percentage_ceiling() {
        let q = Quorum::Percentage(0.5);
        assert_eq!(q.required_count(4), 2);
        assert_eq!(q.required_count(5), 3); // ceil(0.5 * 5) = 3
        assert_eq!(q.required_count(1), 1);
        assert_eq!(q.required_count(0), 0);
    }

    #[test]
    fn quorum_absolute() {
        let q = Quorum::Absolute(3);
        assert_eq!(q.required_count(10), 3);
        assert_eq!(q.required_count(2), 3);
    }

    #[test]
    fn visibility_confirmation_options_defaults() {
        let opts = VisibilityConfirmationOptions::default();
        assert!(matches!(opts.quorum, Quorum::Percentage(p) if (p - 0.5).abs() < f32::EPSILON));
        assert_eq!(opts.timeout_ms, 30_000);
        assert_eq!(opts.sleep_interval_ms, 500);
    }

    #[xmtp_common::test]
    async fn check_node_visibility_times_out_when_no_envelopes() {
        use xmtp_proto::api_client::{ApiBuilder, NetConnectConfig};
        // This test uses a GrpcClient pointed at a non-existent server
        // to verify the timeout/retry behavior.
        let mut builder = xmtp_api_grpc::GrpcClient::builder();
        builder.set_host("http://localhost:1".parse().unwrap());
        let client = builder.build().unwrap();

        let cursor = Cursor::new(1, 1u32);
        let opts = VisibilityConfirmationOptions {
            timeout_ms: 1_000, // 1 second timeout
            sleep_interval_ms: 200,
            ..Default::default()
        };

        let result = check_node_visibility(&client, "test_inbox", &[0u8; 32], cursor, &opts).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::RegistrationNotVisible { failed_nodes } => {
                assert_eq!(failed_nodes, vec![1u32]);
            }
            other => panic!("Expected RegistrationNotVisible, got: {:?}", other),
        }
    }
}
