//! A Restored placeholder with a 16-byte group id is imported history. A
//! 16-byte id names no topic, and a request that named its 17-byte topic
//! would fail as a whole. No sync, stream, push upload, or commit-log turn
//! selects the placeholder, so no request names a 17-byte topic. Catch-up
//! and a reader may select it, or a 16-byte id with no row, and still send
//! no such topic. A placeholder takes no membership commit.
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
    xmtp::mls::database::{Task as TaskProto, task::Task as TaskKind},
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

/// Store an imported 16-byte placeholder, as an archive import does.
fn insert_placeholder(
    client: &crate::utils::FullXmtpClient,
    id: [u8; 16],
) -> Result<GroupId, crate::groups::GroupError> {
    let legacy = GroupId::from(id);
    MlsGroup::insert(
        &client.context,
        Some(legacy.as_slice()),
        GroupMembershipState::Restored,
        ConversationType::Group,
        PolicySet::default(),
        GroupMetadataOptions::default(),
        None,
        false,
    )?;
    Ok(legacy)
}

/// Every recorded topic is 33 bytes, and none carries `legacy`.
fn assert_no_legacy_topic(named: &Named, legacy: &GroupId) {
    for (request, topic) in named.0.lock().iter() {
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
}

/// Sync, a message stream, the push upload, and a commit-log worker turn
/// each run with a keyed group and an imported 16-byte placeholder in the
/// database. Each names the keyed group's topics, none names a 17-byte
/// topic, and the placeholder stays Restored. The commit-log leg is a
/// positive control only: the worker builds a topic from a group's stored
/// commit-log id, and a placeholder never stores one.
// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn a_legacy_placeholder_is_never_named_by_a_request() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let legacy = insert_placeholder(&alix.client, [0x5a; 16])?;
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

    // Registration resolves a webhook host name, and CI runs this test in a
    // build sandbox with no DNS. The globally reachable PCP anycast address
    // (RFC 7723) needs no lookup and passes the backend's address check.
    client
        .enable_notifications(NotificationConfig::new(NotificationChannel::Http {
            url: "https://192.0.0.9/xmtp-notification-test".into(),
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
    assert_no_legacy_topic(&named, &legacy);
    assert_eq!(
        client.db().find_group(&legacy)?.unwrap().membership_state,
        GroupMembershipState::Restored
    );
    client.disable_notifications().await?;
}

/// Catch-up selects every stored group, the placeholder included. The
/// placeholder's obligation completes as inactive without a request, so the
/// keyed group and the Welcome topic in the same head query still reach
/// their targets, and catch-up completes.
// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn catch_up_completes_without_naming_a_legacy_placeholder() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let legacy = insert_placeholder(&alix.client, [0x5a; 16])?;
    let (client, named) = naming_client(&alix.client).await;

    let summary = client
        .catch_up_to_live(Some(Duration::from_secs(10)))
        .await?;

    assert!(summary.completed);
    assert!(
        named
            .by(Request::QueryNewest)
            .contains(&Topic::new_group_message(group.group_id).cloned_vec()),
        "catch-up captures the keyed group's head"
    );
    assert_no_legacy_topic(&named, &legacy);
}

/// A reader that selects a keyed group and a 16-byte id with no stored row
/// delivers the keyed group's messages. The 16-byte id names no topic, so it
/// never reaches the shared stream, which would otherwise fail for every
/// scope. The stream opens with one Subscribe update that names every
/// interested topic, so once the keyed group's topic is sent, a 17-byte
/// topic would already be recorded.
// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn a_reader_that_selects_an_unstored_legacy_id_still_delivers() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let unstored = GroupId::from([0x6b; 16]);
    let (client, named) = naming_client(&alix.client).await;
    let group_topic = Topic::new_group_message(group.group_id).cloned_vec();

    let stream = crate::subscriptions::stream_messages::StreamGroupMessages::new(
        &client.context,
        vec![group.group_id, unstored],
    )
    .await?;
    futures::pin_mut!(stream);
    xmtp_common::wait_for_eq(
        || async { named.by(Request::Subscribe).contains(&group_topic) },
        true,
    )
    .await?;
    assert_no_legacy_topic(&named, &unstored);

    // A refused Subscribe ends the shared stream with an error, so the
    // second read also fails if the backend was asked for a 17-byte topic.
    // Bo's first send can also commit, and the stream yields that update
    // too, so each read skips to the sent content.
    bo.sync_welcomes().await?;
    let sent = bo.group(&group.group_id)?;
    for content in [b"first".as_slice(), b"second".as_slice()] {
        sent.send_message(content, Default::default()).await?;
        xmtp_common::time::timeout(Duration::from_secs(20), async {
            loop {
                let received = stream.next().await.expect("the stream stays open")?;
                assert_eq!(received.group_id, group.group_id);
                if received.decrypted_message_bytes == content {
                    return Ok::<_, crate::subscriptions::SubscribeError>(());
                }
            }
        })
        .await??;
    }
    assert_no_legacy_topic(&named, &unstored);
}

/// A new installation does not reach a placeholder. Device sync schedules
/// no membership commit for it, and a direct call is refused as inactive
/// before it queues an intent, so no request names its topic.
// verifies: ARCH-015
#[xmtp_common::test(unwrap_try = true)]
async fn a_legacy_placeholder_takes_no_membership_commit() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let legacy = insert_placeholder(&alix.client, [0x5a; 16])?;
    tester!(_alix2, from: alix, disable_workers);
    let (client, named) = naming_client(&alix.client).await;

    client
        .device_sync_client()
        .schedule_add_installations_to_groups()?;
    let scheduled: Vec<Vec<u8>> = client
        .db()
        .get_tasks()?
        .iter()
        .filter_map(
            |task| match TaskProto::decode(task.data.as_slice()).ok()?.task {
                Some(TaskKind::AddMissingInstallations(add)) => Some(add.group_id),
                _ => None,
            },
        )
        .collect();
    assert!(scheduled.contains(&group.group_id.to_vec()));
    assert!(!scheduled.contains(&legacy.to_vec()));

    let result = client.group(&legacy)?.add_missing_installations().await;

    assert!(
        matches!(result, Err(crate::groups::GroupError::GroupInactive)),
        "{result:?}"
    );
    assert!(
        client
            .db()
            .find_group_intents(legacy, None, None)?
            .is_empty()
    );
    assert_no_legacy_topic(&named, &legacy);
}
