//! XmtpQuery allows accessing the network while bypassing any local cursor cache.
use std::collections::HashMap;

use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_proto::types::Cursor;

use super::*;

// XMTP Query queries the network for any envelopes
/// matching the cursor criteria given.
#[xmtp_common::async_trait]
pub trait XmtpQuery: MaybeSend + MaybeSync {
    type Error: RetryableError + 'static;
    /// Query every [`Topic`] at [`GlobalCursor`]
    async fn query_at(
        &self,
        topic: Topic,
        at: Option<GlobalCursor>,
    ) -> Result<XmtpEnvelope, Self::Error>;

    /// Whether this client is connected to a d14n network.
    /// V3 clients return `false` (default). D14n clients return `true`.
    /// MigrationClients check the migration cutover state.
    fn is_d14n(&self) -> Result<bool, Self::Error> {
        Ok(false)
    }

    /// Return per-node gRPC client instances for direct node queries.
    /// D14n implementations call GetNodes and build a client per node.
    /// V3/other implementations return an empty map.
    async fn get_node_clients(
        &self,
    ) -> Result<HashMap<u32, xmtp_api_grpc::GrpcClient>, Self::Error> {
        Ok(HashMap::new())
    }
}

// hides implementation detail of XmtpEnvelope/traits
/// Envelopes from the XMTP Network received from a general [`XmtpQuery`]
pub struct XmtpEnvelope {
    inner: Box<dyn EnvelopeCollection<'static>>,
}

impl XmtpEnvelope {
    pub fn new(envelope: impl EnvelopeCollection<'static> + 'static) -> Self {
        Self {
            inner: Box::new(envelope) as Box<_>,
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn cursors(&self) -> Result<Vec<Cursor>, EnvelopeError> {
        self.inner.cursors()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn group_messages(&self) -> Result<Vec<GroupMessage>, EnvelopeError> {
        Ok(self.inner.group_messages()?.into_iter().flatten().collect())
    }

    pub fn welcome_messages(&self) -> Result<Vec<WelcomeMessage>, EnvelopeError> {
        Ok(self
            .inner
            .welcome_messages()?
            .into_iter()
            .flatten()
            .collect())
    }

    pub fn client_envelopes(&self) -> Result<Vec<ClientEnvelope>, EnvelopeError> {
        self.inner.client_envelopes()
    }
}
