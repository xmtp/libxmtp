//! A Restored placeholder with a 16-byte group id is imported history. A
//! 16-byte id names no topic, and a request that named its 17-byte topic
//! would fail as a whole. No sync, stream, push upload, or commit-log turn
//! selects the placeholder, so no request names a 17-byte topic.
use crate::{
    client::notifications::{NotificationChannel, NotificationConfig},
    context::XmtpSharedContext,
    groups::{
        MlsGroup, PolicySet,
        commit_log::{CommitLogTestFunction, CommitLogWorker},
    },
    tester,
    worker::WorkerConfig,
};
use futures::StreamExt;
use parking_lot::Mutex;
use prost::{Message, bytes::Bytes};
use std::sync::Arc;
use xmtp_common::time::Duration;
use xmtp_db::{
    consent_record::ConsentState,
    group::{ConversationType, GroupMembershipState},
    prelude::*,
};
use xmtp_mls_common::group::GroupMetadataOptions;
use xmtp_proto::{
    api::{ApiClientError, ArcClient, BytesStream, Client as Transport, IsConnectedCheck},
    backend_v1 as wire,
    types::{GroupId, Topic},
};

/// The request that named a topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Request {
    Query,
    QueryNewest,
    SubscribeStatic,
    Subscribe,
    UpdateSubscriptions,
}

/// Every topic each request named, in order.
#[derive(Default)]
struct Named(Mutex<Vec<(Request, Vec<u8>)>>);

impl Named {
    fn record(&self, path: &str, body: &[u8]) {
        let topics: Vec<(Request, Vec<u8>)> = match path {
            "/xmtp.backend.v1.QueryService/Query" => wire::QueryRequest::decode(body)
                .map(|request| {
                    request
                        .queries
                        .into_iter()
                        .filter_map(|query| query.topic)
                        .map(|topic| (Request::Query, topic.topic))
                        .collect()
                })
                .unwrap_or_default(),
            "/xmtp.backend.v1.QueryService/QueryNewest" => wire::QueryNewestRequest::decode(body)
                .map(|request| {
                    request
                        .topics
                        .into_iter()
                        .map(|topic| (Request::QueryNewest, topic.topic))
                        .collect()
                })
                .unwrap_or_default(),
            "/xmtp.backend.v1.SubscriptionService/SubscribeStatic" => {
                wire::SubscribeStaticRequest::decode(body)
                    .map(|request| {
                        request
                            .topics
                            .into_iter()
                            .filter_map(|query| query.topic)
                            .map(|topic| (Request::SubscribeStatic, topic.topic))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            "/xmtp.backend.v1.SubscriptionService/Subscribe" => {
                match wire::SubscribeRequest::decode(body).map(|frame| frame.request) {
                    Ok(Some(wire::subscribe_request::Request::Update(update))) => update
                        .adds
                        .into_iter()
                        .filter_map(|query| query.topic)
                        .chain(update.removes)
                        .map(|topic| (Request::Subscribe, topic.topic))
                        .collect(),
                    _ => vec![],
                }
            }
            "/xmtp.backend.v1.NotificationService/UpdateSubscriptions" => {
                wire::UpdateSubscriptionsRequest::decode(body)
                    .map(|request| {
                        request
                            .adds
                            .iter()
                            .map(|add| add.topic.clone())
                            .chain(request.removes.iter().cloned())
                            .map(|topic| (Request::UpdateSubscriptions, topic))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            _ => vec![],
        };
        self.0.lock().extend(topics);
    }

    fn by(&self, request: Request) -> Vec<Vec<u8>> {
        self.0
            .lock()
            .iter()
            .filter(|(seen, _)| *seen == request)
            .map(|(_, topic)| topic.clone())
            .collect()
    }
}

/// A transport that forwards every request to the test backend and records
/// the topics each one names.
struct NamingTransport {
    inner: ArcClient,
    named: Arc<Named>,
}

#[xmtp_common::async_trait]
impl Transport for NamingTransport {
    fn host(&self) -> &str {
        self.inner.host()
    }

    fn has_credential_source(&self) -> bool {
        self.inner.has_credential_source()
    }

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError> {
        self.named.record(path.path(), &body);
        self.inner.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        self.named.record(path.path(), &body);
        self.inner.stream(request, path, body).await
    }

    async fn bidi_stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: xmtp_common::BoxDynStream<'static, Bytes>,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        let named = self.named.clone();
        let name = path.path().to_owned();
        let body = body
            .inspect(move |frame| named.record(&name, frame))
            .boxed();
        self.inner.bidi_stream(request, path, body).await
    }
}

#[xmtp_common::async_trait]
impl IsConnectedCheck for NamingTransport {
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}

/// The same client, database, and identity as `base`, with every backend
/// request going through a [`NamingTransport`]. Its workers are stopped, so
/// only the operations a test runs send requests.
async fn naming_client(
    base: &crate::utils::FullXmtpClient,
) -> (crate::utils::FullXmtpClient, Arc<Named>) {
    use xmtp_proto::api::ToBoxedClient;
    let named = Arc::new(Named::default());
    let backend = base.context.api().api_client.raw_for_test().inner();
    let transport = NamingTransport {
        inner: backend.inner().clone(),
        named: named.clone(),
    };
    let api = xmtp_api_backend::BackendClient::new(transport.arced())
        .with_auth_handle(backend.auth_handle());
    let client = crate::builder::ClientBuilder::from_client(base.clone())
        .api_client_with_streams(Arc::new(xmtp_api_backend::TrackedStatsClient::new(api)))
        .with_disable_workers(false)
        .worker_config(WorkerConfig::default())
        .with_allow_offline(Some(true))
        .build()
        .await
        .expect("a naming client");
    client.workers.shutdown().await;
    (client, named)
}

/// Sync, a message stream, the push upload, and a commit-log worker turn
/// each run with a keyed group and an imported 16-byte placeholder in the
/// database. Each names the keyed group's topics, none names a 17-byte
/// topic, and the placeholder stays Restored.
// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn a_legacy_placeholder_is_never_named_by_a_request() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let legacy = GroupId::from([0x5a; 16]);
    MlsGroup::insert(
        &alix.context,
        Some(legacy.as_slice()),
        GroupMembershipState::Restored,
        ConversationType::Group,
        PolicySet::default(),
        GroupMetadataOptions::default(),
        None,
        false,
    )?;
    // An archive can carry the conversation's consent, and an allowed
    // conversation is one the push upload would otherwise select.
    alix.group(&legacy)?
        .update_consent_state(ConsentState::Allowed)?;
    let (client, named) = naming_client(&alix.client).await;

    client.sync_all_welcomes_and_groups(None).await?;

    let stream = client.stream_all_messages(None, None).await?;
    futures::pin_mut!(stream);
    bo.sync_welcomes().await?;
    bo.group(&group.group_id)?
        .send_message(b"over the stream", Default::default())
        .await?;
    let received = xmtp_common::time::timeout(Duration::from_secs(20), stream.next())
        .await?
        .expect("the stream yields the message")?;
    assert_eq!(received.group_id, group.group_id);

    client
        .enable_notifications(NotificationConfig::new(NotificationChannel::Http {
            url: "https://example.com/xmtp-notification-test".into(),
            signing_key: vec![7; 32].into(),
        }))
        .await?;
    crate::worker::notifications::run(&client.context).await?;

    CommitLogWorker::new(client.context.clone())
        .run_test(CommitLogTestFunction::All, None)
        .await?;

    let group_topic = Topic::new_group_message(group.group_id).cloned_vec();
    let commit_log_topic = Topic::new_commit_log(
        client
            .db()
            .find_group(&group.group_id)?
            .and_then(|stored| stored.commit_log_public_key)
            .expect("a keyed group stores its commit-log id"),
    )
    .cloned_vec();
    let streamed = [
        named.by(Request::Subscribe),
        named.by(Request::SubscribeStatic),
    ]
    .concat();
    assert!(named.by(Request::Query).contains(&group_topic), "sync");
    assert!(streamed.contains(&group_topic), "stream");
    assert!(
        named
            .by(Request::UpdateSubscriptions)
            .contains(&group_topic),
        "push upload"
    );
    assert!(
        named.by(Request::Query).contains(&commit_log_topic),
        "commit-log read"
    );
    let all = named.0.lock().clone();
    for (request, topic) in &all {
        assert_eq!(
            topic.len(),
            33,
            "{request:?} named a {}-byte topic",
            topic.len()
        );
        assert_ne!(
            &topic[1..17],
            legacy.as_slice(),
            "{request:?} named the placeholder"
        );
    }
    assert_eq!(
        client.db().find_group(&legacy)?.unwrap().membership_state,
        GroupMembershipState::Restored
    );
    client.disable_notifications().await?;
}
