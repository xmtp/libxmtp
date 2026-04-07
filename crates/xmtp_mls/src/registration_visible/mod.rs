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
    /// Create a percentage quorum, clamping to `[0.0, 1.0]`.
    /// NaN is treated as 0.0.
    pub fn percentage(p: f32) -> Self {
        let p = if p.is_nan() { 0.0 } else { p.clamp(0.0, 1.0) };
        Self::Percentage(p)
    }

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
}

impl Default for VisibilityConfirmationOptions {
    fn default() -> Self {
        Self {
            quorum: Quorum::Absolute(1),
            timeout_ms: 30_000,
        }
    }
}

/// Perform a single query against one node to check whether both the
/// identity-update envelope and the key-package envelope are visible.
///
/// `topics` should be pre-built from the inbox_id and installation_id
/// to avoid re-computing on every retry attempt.
pub(crate) async fn check_node_visibility<C: Client>(
    node_client: &C,
    node_id: u32,
    topics: &[Vec<u8>],
    cursor: Cursor,
) -> Result<(), ClientError> {
    let mut endpoint = QueryEnvelopes::builder()
        .envelopes(EnvelopesQuery {
            topics: topics.to_vec(),
            originator_node_ids: vec![],
            last_seen: None,
        })
        .build()
        .map_err(|e| {
            ClientError::Generic(format!("failed to build QueryEnvelopes endpoint: {e}"))
        })?;

    let response = endpoint.query(node_client).await.map_err(|e| {
        tracing::warn!(node_id, error = %e, "check_node_visibility: API error querying node");
        ClientError::EnvelopesNotYetVisible { node_id }
    })?;

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
        Ok(())
    } else {
        Err(ClientError::EnvelopesNotYetVisible { node_id })
    }
}

impl<Context> XmtpClient<Context>
where
    Context: XmtpSharedContext,
    Context::ApiClient: XmtpApi + XmtpQuery,
{
    fn check_is_ready(&self) -> Result<(), ClientError> {
        if self.identity().is_ready() {
            Ok(())
        } else {
            Err(ClientError::RegistrationNotVisible {
                failed_nodes: vec![],
            })
        }
    }

    fn load_registration_cursor(&self) -> Result<Option<Cursor>, ClientError> {
        let stored_identity: Option<StoredIdentity> =
            self.context.db().fetch(&()).map_err(ClientError::Storage)?;

        Ok(stored_identity.and_then(|si| {
            match (
                si.registration_cursor_originator_id,
                si.registration_cursor_sequence_id,
            ) {
                (Some(orig_id), Some(seq_id)) => Some(Cursor::new(seq_id as u64, orig_id as u32)),
                _ => None,
            }
        }))
    }

    async fn poll_node_quorum(
        &self,
        cursor: Cursor,
        options: &VisibilityConfirmationOptions,
    ) -> Result<(), ClientError> {
        let node_clients = self
            .context
            .api()
            .get_node_clients()
            .await
            .map_err(|e| ClientError::Api(xmtp_api::dyn_err(e)))?;

        if node_clients.is_empty() {
            tracing::warn!("get_node_clients returned empty map; falling back to is_ready check");
            return self.check_is_ready();
        }

        let total_nodes = node_clients.len();
        let mut required = options.quorum.required_count(total_nodes);
        if required == 0 {
            tracing::warn!("quorum resolved to 0; requiring at least 1 node");
            required = 1;
        } else if required > total_nodes {
            tracing::warn!(
                required,
                total_nodes,
                "quorum exceeds node count; clamping to node count"
            );
            required = total_nodes;
        }

        use xmtp_proto::types::Topic;

        let inbox_id_bytes = hex::decode(self.inbox_id())
            .map_err(|e| ClientError::Generic(format!("invalid hex inbox_id: {e}")))?;
        let identity_topic = Topic::new_identity_update(&inbox_id_bytes);
        let key_package_topic = Topic::new_key_package(self.installation_public_key());
        let topics = vec![identity_topic.cloned_vec(), key_package_topic.cloned_vec()];

        let retry = xmtp_common::Retry::builder()
            .retries(10)
            .with_strategy(
                xmtp_common::ExponentialBackoff::builder()
                    .total_wait_max(xmtp_common::time::Duration::from_millis(options.timeout_ms))
                    .build(),
            )
            .build();

        let futures: FuturesUnordered<_> = node_clients
            .into_iter()
            .map(|(node_id, client)| {
                let topics = topics.clone();
                let retry = retry.clone();
                async move {
                    let result = xmtp_common::retry_async!(
                        retry,
                        (async { check_node_visibility(&client, node_id, &topics, cursor).await })
                    );
                    let result = result.map_err(|_| ClientError::RegistrationNotVisible {
                        failed_nodes: vec![node_id],
                    });
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
                    if total_nodes - failed_nodes.len() < required {
                        return Err(ClientError::RegistrationNotVisible { failed_nodes });
                    }
                }
            }
        }

        Err(ClientError::RegistrationNotVisible { failed_nodes })
    }

    /// Wait until the registration for this client is visible on the network.
    ///
    /// Always checks `is_ready()` first. For D14n clients, additionally queries
    /// each node directly and waits until a quorum confirms the identity-update
    /// and key-package envelopes are visible.
    pub async fn wait_for_registration_visible(
        &self,
        options: VisibilityConfirmationOptions,
    ) -> Result<(), ClientError> {
        self.check_is_ready()?;

        let is_d14n = self
            .context
            .api()
            .is_d14n()
            .map_err(|e| ClientError::Api(xmtp_api::dyn_err(e)))?;

        if !is_d14n {
            return Ok(());
        }

        let Some(cursor) = self.load_registration_cursor()? else {
            tracing::warn!(
                "d14n client has no registration cursor (likely registered before migration); \
                 skipping node visibility check"
            );
            return Ok(());
        };

        self.poll_node_quorum(cursor, &options).await
    }
}

#[cfg(test)]
mod tests;
