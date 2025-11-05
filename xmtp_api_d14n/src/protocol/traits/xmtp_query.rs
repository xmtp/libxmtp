//! XmtpQuery allows accessing the network while bypassing any local cursor cache.
use xmtp_common::{MaybeSend, MaybeSync};

use super::*;

// XMTP Query queries the network for any envelopes
/// matching the cursor criteria given.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpQuery: MaybeSend + MaybeSync {
    type Error: RetryableError + 'static;
    /// Query every [`Topic`] at [`GlobalCursor`]
    async fn query_at(
        &self,
        topic: Topic,
        at: Option<GlobalCursor>,
    ) -> Result<XmtpEnvelope, Self::Error>;
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
}
