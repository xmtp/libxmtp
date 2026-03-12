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

#[cfg(test)]
mod tests {
    use super::super::tests::{FakeNetworkError, TestNetworkClient, build_test_client};
    use super::*;
    use crate::protocol::InMemoryCursorStore;
    use futures::StreamExt;

    fn migration_error() -> ApiClientError {
        ApiClientError::client(FakeNetworkError(
            "XMTP V3 streaming is no longer available. Please upgrade your client to XMTP D14N."
                .to_string(),
        ))
    }

    fn other_error() -> ApiClientError {
        ApiClientError::client(FakeNetworkError("some unrelated error".to_string()))
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_fallback_yields_v3_items_without_error() {
        let store = InMemoryCursorStore::new();
        let client = build_test_client(TestNetworkClient::new(), TestNetworkClient::new(), store);

        let v3_stream: BoxDynStream<'static, Result<i32, ApiClientError>> =
            Box::pin(futures::stream::iter(vec![Ok(1), Ok(2), Ok(3)]));

        let fallback_stream = client.with_d14n_fallback(v3_stream, || async {
            panic!("d14n factory should not be called when v3 stream succeeds")
        });
        futures::pin_mut!(fallback_stream);

        let items: Vec<i32> = fallback_stream.map(|r| r.unwrap()).collect().await;

        assert_eq!(items, vec![1, 2, 3]);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_fallback_switches_on_migration_error() {
        let store = InMemoryCursorStore::new();
        let client = build_test_client(TestNetworkClient::new(), TestNetworkClient::new(), store);

        let v3_stream: BoxDynStream<'static, Result<i32, ApiClientError>> =
            Box::pin(futures::stream::iter(vec![Ok(1), Err(migration_error())]));

        let fallback_stream = client.with_d14n_fallback(v3_stream, || async {
            let d14n: BoxDynStream<'static, Result<i32, ApiClientError>> =
                Box::pin(futures::stream::iter(vec![Ok(10), Ok(11)]));
            Ok(d14n)
        });
        futures::pin_mut!(fallback_stream);

        let item = fallback_stream.next().await.unwrap();
        assert_eq!(item.unwrap(), 1);

        let item = fallback_stream.next().await.unwrap();
        assert!(item.is_err());

        let item = fallback_stream.next().await.unwrap();
        assert_eq!(item.unwrap(), 10);

        let item = fallback_stream.next().await.unwrap();
        assert_eq!(item.unwrap(), 11);

        assert!(fallback_stream.next().await.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_fallback_does_not_switch_on_non_migration_error() {
        let store = InMemoryCursorStore::new();
        let client = build_test_client(TestNetworkClient::new(), TestNetworkClient::new(), store);

        let v3_stream: BoxDynStream<'static, Result<i32, ApiClientError>> =
            Box::pin(futures::stream::iter(vec![Ok(1), Err(other_error())]));

        let fallback_stream = client.with_d14n_fallback(v3_stream, || async {
            panic!("d14n factory should not be called for non-migration errors")
        });
        futures::pin_mut!(fallback_stream);

        let item = fallback_stream.next().await.unwrap();
        assert_eq!(item.unwrap(), 1);

        let item = fallback_stream.next().await.unwrap();
        assert!(item.is_err());

        assert!(fallback_stream.next().await.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_fallback_empty_v3_stream() {
        let store = InMemoryCursorStore::new();
        let client = build_test_client(TestNetworkClient::new(), TestNetworkClient::new(), store);

        let v3_stream: BoxDynStream<'static, Result<i32, ApiClientError>> =
            Box::pin(futures::stream::empty());

        let fallback_stream = client.with_d14n_fallback(v3_stream, || async {
            panic!("d14n factory should not be called for empty stream")
        });
        futures::pin_mut!(fallback_stream);

        assert!(fallback_stream.next().await.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_fallback_d14n_factory_error() {
        let store = InMemoryCursorStore::new();
        let client = build_test_client(TestNetworkClient::new(), TestNetworkClient::new(), store);

        let v3_stream: BoxDynStream<'static, Result<i32, ApiClientError>> =
            Box::pin(futures::stream::iter(vec![Err(migration_error())]));

        let fallback_stream = client.with_d14n_fallback(v3_stream, || async {
            Err::<BoxDynStream<'static, Result<i32, ApiClientError>>, _>(other_error())
        });
        futures::pin_mut!(fallback_stream);

        let item = fallback_stream.next().await.unwrap();
        assert!(item.is_err());

        let item = fallback_stream.next().await.unwrap();
        assert!(item.is_err());

        assert!(fallback_stream.next().await.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_fallback_migration_error_mid_stream() {
        let store = InMemoryCursorStore::new();
        let client = build_test_client(TestNetworkClient::new(), TestNetworkClient::new(), store);

        let v3_stream: BoxDynStream<'static, Result<i32, ApiClientError>> =
            Box::pin(futures::stream::iter(vec![
                Ok(1),
                Ok(2),
                Ok(3),
                Err(migration_error()),
                Ok(99),
            ]));

        let fallback_stream = client.with_d14n_fallback(v3_stream, || async {
            let d14n: BoxDynStream<'static, Result<i32, ApiClientError>> =
                Box::pin(futures::stream::iter(vec![Ok(20), Ok(21)]));
            Ok(d14n)
        });
        futures::pin_mut!(fallback_stream);

        assert_eq!(fallback_stream.next().await.unwrap().unwrap(), 1);
        assert_eq!(fallback_stream.next().await.unwrap().unwrap(), 2);
        assert_eq!(fallback_stream.next().await.unwrap().unwrap(), 3);

        let item = fallback_stream.next().await.unwrap();
        assert!(item.is_err());

        assert_eq!(fallback_stream.next().await.unwrap().unwrap(), 20);
        assert_eq!(fallback_stream.next().await.unwrap().unwrap(), 21);

        assert!(fallback_stream.next().await.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_fallback_multiple_non_migration_errors_no_switch() {
        let store = InMemoryCursorStore::new();
        let client = build_test_client(TestNetworkClient::new(), TestNetworkClient::new(), store);

        let v3_stream: BoxDynStream<'static, Result<i32, ApiClientError>> =
            Box::pin(futures::stream::iter(vec![
                Ok(1),
                Err(other_error()),
                Ok(2),
                Err(other_error()),
            ]));

        let fallback_stream = client.with_d14n_fallback(v3_stream, || async {
            panic!("d14n factory should not be called for non-migration errors")
        });
        futures::pin_mut!(fallback_stream);

        assert_eq!(fallback_stream.next().await.unwrap().unwrap(), 1);
        assert!(fallback_stream.next().await.unwrap().is_err());
        assert_eq!(fallback_stream.next().await.unwrap().unwrap(), 2);
        assert!(fallback_stream.next().await.unwrap().is_err());
        assert!(fallback_stream.next().await.is_none());
    }
}
