use crate::{V3Client, v3::*};
use xmtp_api_grpc::error::GrpcError;
use xmtp_api_grpc::streams::{TryFromItem, try_from_stream};
use xmtp_configuration::Originators;
use xmtp_proto::api::{ApiClientError, Client, QueryStream, XmtpStream};
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::mls_v1::subscribe_group_messages_request::Filter as GroupSubscribeFilter;
use xmtp_proto::mls_v1::subscribe_welcome_messages_request::Filter as WelcomeSubscribeFilter;
use xmtp_proto::types::{GroupId, GroupMessage, InstallationId, TopicKind, WelcomeMessage};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> XmtpMlsStreams for V3Client<C>
where
    C: Send + Sync + Client<Error = GrpcError>,
{
    type GroupMessageStream =
        TryFromItem<XmtpStream<<C as Client>::Stream, V3ProtoGroupMessage>, GroupMessage>;

    type WelcomeMessageStream =
        TryFromItem<XmtpStream<<C as Client>::Stream, V3ProtoWelcomeMessage>, WelcomeMessage>;

    type Error = ApiClientError<GrpcError>;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        let mut filters = vec![];
        let topics = group_ids
            .into_iter()
            .map(|gid| TopicKind::GroupMessagesV1.create(gid))
            .collect::<Vec<_>>();
        for topic in topics {
            let cursors = self.cursor_store.latest_for_each(
                &[Originators::APPLICATION_MESSAGES, Originators::MLS_COMMITS],
                &topic,
            )?;
            let id_cursor = cursors
                .into_iter()
                .map(|c| c.sequence_id)
                .max()
                .unwrap_or(0);
            filters.push(GroupSubscribeFilter {
                group_id: topic.identifier().to_vec(),
                id_cursor,
            })
        }
        Ok(try_from_stream(
            SubscribeGroupMessages::builder()
                .filters(filters)
                .build()?
                .stream(&self.client)
                .await?,
        ))
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        let mut filters = vec![];
        let topics = installations
            .into_iter()
            .map(|id| TopicKind::WelcomeMessagesV1.create(id))
            .collect::<Vec<_>>();
        for topic in topics {
            let id_cursor = self
                .cursor_store
                .latest(Originators::WELCOME_MESSAGES, &topic)?;
            filters.push(WelcomeSubscribeFilter {
                installation_key: topic.identifier().to_vec(),
                id_cursor: id_cursor.sequence_id,
            })
        }

        Ok(try_from_stream(
            SubscribeWelcomeMessages::builder()
                .filters(filters)
                .build()?
                .stream(&self.client)
                .await?,
        ))
    }
}
