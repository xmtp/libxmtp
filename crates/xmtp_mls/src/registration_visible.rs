use futures::stream::{FuturesUnordered, StreamExt};
use xmtp_api::XmtpApi;
use xmtp_api_d14n::d14n::QueryEnvelopes;
use xmtp_api_d14n::protocol::traits::Envelope;
use xmtp_api_d14n::protocol::traits::XmtpQuery;
use xmtp_db::{identity::StoredIdentity, prelude::*};
use xmtp_proto::api::{Client, Query};
use xmtp_proto::types::{Cursor, TopicKind};
use xmtp_proto::xmtp::xmtpv4::message_api::EnvelopesQuery;

use crate::client::{Client as XmtpClient, ClientError};
use crate::context::XmtpSharedContext;

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

impl<Context> XmtpClient<Context>
where
    Context: XmtpSharedContext,
    Context::ApiClient: XmtpApi + XmtpQuery,
{
    /// Wait until the registration for this client is visible on the network.
    ///
    /// For V3 clients (no cursor stored), falls back to checking `is_ready()`.
    /// For D14n clients, queries each node directly and waits until a quorum
    /// confirms the identity-update and key-package envelopes are visible.
    pub async fn wait_for_registration_visible(
        &self,
        options: VisibilityConfirmationOptions,
    ) -> Result<(), ClientError> {
        // Load cursor from stored identity
        let stored_identity: Option<StoredIdentity> =
            self.context.db().fetch(&()).map_err(ClientError::Storage)?;

        let cursor = stored_identity.and_then(|si| {
            match (
                si.registration_cursor_originator_id,
                si.registration_cursor_sequence_id,
            ) {
                (Some(orig_id), Some(seq_id)) => Some(Cursor::new(seq_id as u64, orig_id as u32)),
                _ => None,
            }
        });

        // V3 path: no cursor stored — fall back to is_ready check
        let Some(cursor) = cursor else {
            if self.identity().is_ready() {
                return Ok(());
            } else {
                return Err(ClientError::RegistrationNotVisible {
                    failed_nodes: vec![],
                });
            }
        };

        // D14n path: get per-node clients via the API
        let node_clients = self
            .context
            .api()
            .get_node_clients()
            .await
            .map_err(|e| ClientError::Generic(format!("failed to get node clients: {e}")))?;

        if node_clients.is_empty() {
            // No per-node access available — fall back to is_ready
            tracing::warn!("get_node_clients returned empty map; falling back to is_ready check");
            if self.identity().is_ready() {
                return Ok(());
            } else {
                return Err(ClientError::RegistrationNotVisible {
                    failed_nodes: vec![],
                });
            }
        }

        let total_nodes = node_clients.len();
        let mut required = options.quorum.required_count(total_nodes);
        if required > total_nodes {
            tracing::warn!(
                required,
                total_nodes,
                "quorum exceeds node count; clamping to node count"
            );
            required = total_nodes;
        }

        let inbox_id = self.inbox_id().to_string();
        let installation_id: Vec<u8> = self.installation_public_key().to_vec();

        // Spawn concurrent futures per node
        let futures: FuturesUnordered<_> = node_clients
            .into_iter()
            .map(|(node_id, client)| {
                let inbox_id = inbox_id.clone();
                let installation_id = installation_id.clone();
                let opts = options.clone();
                async move {
                    let result =
                        check_node_visibility(&client, &inbox_id, &installation_id, cursor, &opts)
                            .await;
                    (node_id, result)
                }
            })
            .collect();

        let mut confirmed = 0usize;
        let mut failed_nodes: Vec<u32> = Vec::new();

        futures::pin_mut!(futures);
        while let Some((node_id, result)) = futures.next().await {
            match result {
                Ok(()) => {
                    confirmed += 1;
                    if confirmed >= required {
                        return Ok(());
                    }
                }
                Err(_) => {
                    failed_nodes.push(node_id);
                }
            }
        }

        Err(ClientError::RegistrationNotVisible { failed_nodes })
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
