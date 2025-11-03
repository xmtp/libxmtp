use crate::TryExtractorStream;
use crate::d14n::SubscribeEnvelopes;
use crate::protocol::{GroupMessageExtractor, WelcomeMessageExtractor};
use crate::queries::stream;

use super::D14nClient;
use std::collections::HashMap;
use xmtp_common::RetryableError;
use xmtp_proto::api::{ApiClientError, Client, QueryStream, XmtpStream};
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::types::{GlobalCursor, GroupId, InstallationId, TopicKind};
use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeEnvelopesResponse;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, G, E> XmtpMlsStreams for D14nClient<C, G>
where
    C: Client<Error = E>,
    <C as Client>::Stream: 'static,
    G: Client<Error = E>,
    E: RetryableError + 'static,
{
    type Error = ApiClientError<E>;

    type GroupMessageStream = TryExtractorStream<
        XmtpStream<<C as Client>::Stream, SubscribeEnvelopesResponse>,
        GroupMessageExtractor,
    >;

    type WelcomeMessageStream = TryExtractorStream<
        XmtpStream<<C as Client>::Stream, SubscribeEnvelopesResponse>,
        WelcomeMessageExtractor,
    >;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        let topics = group_ids
            .iter()
            .map(|gid| TopicKind::GroupMessagesV1.create(gid))
            .collect::<Vec<_>>();
        let lcc = self
            .cursor_store
            .load()
            .lcc_maybe_missing(&topics.iter().collect::<Vec<_>>())?;
        tracing::debug!("subscribing to messages @cursor={}", lcc);
        let s = SubscribeEnvelopes::builder()
            .topics(topics)
            .last_seen(lcc)
            .build()?
            .stream(&self.message_client)
            .await?;
        Ok(stream::try_extractor(s))
    }

    async fn subscribe_group_messages_with_cursors(
        &self,
        groups_with_cursors: &[(&GroupId, GlobalCursor)],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        let topics = groups_with_cursors
            .iter()
            .map(|(gid, _)| TopicKind::GroupMessagesV1.create(gid))
            .collect::<Vec<_>>();

        // Compute the lowest common cursor from the provided cursors
        let mut min_clock: HashMap<u32, u64> = HashMap::new();
        for (_, cursor) in groups_with_cursors {
            for (&node_id, &seq_id) in cursor.iter() {
                min_clock
                    .entry(node_id)
                    .and_modify(|existing| *existing = (*existing).min(seq_id))
                    .or_insert(seq_id);
            }
        }
        let lcc = GlobalCursor::new(min_clock);

        tracing::debug!(
            "subscribing to messages with provided cursors @cursor={}",
            lcc
        );
        let s = SubscribeEnvelopes::builder()
            .topics(topics)
            .last_seen(lcc)
            .build()?
            .stream(&self.message_client)
            .await?;
        Ok(stream::try_extractor(s))
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        let topics = installations
            .iter()
            .map(|ins| TopicKind::WelcomeMessagesV1.create(ins))
            .collect::<Vec<_>>();
        let lcc = self
            .cursor_store
            .load()
            .lowest_common_cursor(&topics.iter().collect::<Vec<_>>())?;
        let s = SubscribeEnvelopes::builder()
            .topics(topics)
            .last_seen(lcc)
            .build()?
            .stream(&self.message_client)
            .await?;
        Ok(stream::try_extractor(s))
    }
}
