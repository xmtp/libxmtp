use crate::MigrationClient;
use crate::protocol::CursorStore;
use crate::queries::combined::is_d14n_migration_error;

use futures::{Stream, StreamExt, TryStreamExt, stream};
use xmtp_common::{BoxDynStream, MaybeSend};
use xmtp_proto::api::{ApiClientError, Client};
use xmtp_proto::api_client::{BoxedGroupS, BoxedWelcomeS, XmtpMlsStreams};
use xmtp_proto::types::{GroupId, InstallationId, TopicCursor};

enum Phase<T> {
    V3(BoxDynStream<'static, Result<T, ApiClientError>>),
    D14n(BoxDynStream<'static, Result<T, ApiClientError>>),
}

impl<V3, D14n, Store> MigrationClient<V3, D14n, Store>
where
    V3: Client,
    D14n: Client,
    Store: CursorStore + Clone,
{
    /// starts a stream, and restarts as a d14n stream if it fails with
    /// migration error.
    fn with_d14n_fallback<F, Fut, T>(
        &self,
        v3_stream: BoxDynStream<'static, Result<T, ApiClientError>>,
        new_stream: F,
    ) -> impl Stream<Item = Result<T, ApiClientError>> + 'static + use<F, Fut, T, V3, D14n, Store>
    where
        F: MaybeSend + Clone + Fn() -> Fut + 'static,
        Fut: MaybeSend
            + Future<
                Output = Result<BoxDynStream<'static, Result<T, ApiClientError>>, ApiClientError>,
            >,
        T: 'static,
    {
        stream::unfold(Phase::V3(v3_stream), move |phase| {
            let value = new_stream.clone();
            async move {
                match phase {
                    Phase::V3(mut s) => match s.next().await {
                        Some(Err(e)) if is_d14n_migration_error(&e) => {
                            let fallback =
                                Box::pin(stream::once(async move { value().await }).try_flatten());
                            Some((Err(e), Phase::D14n(fallback)))
                        }
                        Some(item) => Some((item, Phase::V3(s))),
                        None => None,
                    },
                    Phase::D14n(mut s) => s.next().await.map(|item| (item, Phase::D14n(s))),
                }
            }
        })
    }
}

#[xmtp_common::async_trait]
impl<V3, D14n, Store> XmtpMlsStreams for MigrationClient<V3, D14n, Store>
where
    V3: Client,
    D14n: Client,
    Store: CursorStore + Clone,
{
    type Error = ApiClientError;

    type GroupMessageStream = BoxedGroupS<ApiClientError>;

    type WelcomeMessageStream = BoxedWelcomeS<ApiClientError>;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        if self.has_migrated()? {
            return self.xmtpd_client.subscribe_group_messages(group_ids).await;
        }

        let v3_stream = self
            .choose_client()
            .await?
            .subscribe_group_messages(group_ids)
            .await?;
        let xclient = self.xmtpd_client.clone();
        let ids: Vec<GroupId> = group_ids.iter().map(|&g| g.clone()).collect();
        Ok(Box::pin(self.with_d14n_fallback(v3_stream, move || {
            let c = xclient.clone();
            let ids = ids.clone();
            async move {
                c.subscribe_group_messages(&ids.iter().collect::<Vec<&GroupId>>())
                    .await
            }
        })))
    }

    async fn subscribe_group_messages_with_cursors(
        &self,
        topics: &TopicCursor,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        if self.has_migrated()? {
            return self
                .xmtpd_client
                .subscribe_group_messages_with_cursors(topics)
                .await;
        }

        let v3_stream = self
            .choose_client()
            .await?
            .subscribe_group_messages_with_cursors(topics)
            .await?;
        let xclient = self.xmtpd_client.clone();
        let topics: TopicCursor = topics.clone();
        Ok(Box::pin(self.with_d14n_fallback(v3_stream, move || {
            let xmtpd = xclient.clone();
            let topics = topics.clone();
            async move { xmtpd.subscribe_group_messages_with_cursors(&topics).await }
        })))
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        if self.has_migrated()? {
            return self
                .xmtpd_client
                .subscribe_welcome_messages(installations)
                .await;
        }

        let v3_stream = self
            .choose_client()
            .await?
            .subscribe_welcome_messages(installations)
            .await?;
        let xclient = self.xmtpd_client.clone();
        let installations: Vec<InstallationId> = installations.iter().map(|&i| i.clone()).collect();

        Ok(Box::pin(self.with_d14n_fallback(v3_stream, move || {
            let xmtpd = xclient.clone();
            let installations = installations.clone();
            async move {
                xmtpd
                    .subscribe_welcome_messages(
                        &installations.iter().collect::<Vec<&InstallationId>>(),
                    )
                    .await
            }
        })))
    }
}
