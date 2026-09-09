#![allow(clippy::unwrap_used)]
use super::*;
use crate::queries::{BackendBinding, BidiConnection};
use prost::Message;

use std::sync::{Arc, Mutex};
use std::time::Duration;
use xmtp_proto::backend_v1::subscribe_request::Update as Mutate;
use xmtp_proto::backend_v1::{self, ServerEnvelope, subscribe_response};
type GroupMessage = ServerEnvelope;
type WelcomeMessage = ServerEnvelope;
use xmtp_proto::types::TopicKind;

const WAIT: Duration = Duration::from_secs(5);

use crate::test::bidi::{MockApi, MockServer};

fn mock_pair() -> (MockApi, MockServer) {
    let (api, server) = crate::test::bidi::mock_pair();
    server.send(started(30_000));
    (api, server)
}

type Servers = Arc<Mutex<Vec<MockServer>>>;

fn transport() -> (BidiTransport<BackendBinding>, Servers) {
    transport_born(false)
}

fn transport_born(suspended: bool) -> (BidiTransport<BackendBinding>, Servers) {
    let servers: Servers = Arc::default();
    let sink = servers.clone();
    let transport = BidiTransport::new(
        move |initial| {
            let (api, server) = mock_pair();
            sink.lock().unwrap().push(server);
            async move {
                BidiConnection::open(&api, initial)
                    .await
                    .map_err(OpenError::new)
            }
        },
        suspended,
    );
    (transport, servers)
}

fn transport_capped(
    max_topics: usize,
    max_bytes: usize,
) -> (BidiTransport<BackendBinding>, Servers) {
    let servers: Servers = Arc::default();
    let sink = servers.clone();
    let transport = BidiTransport::new_with_chunk_limits(
        move |initial| {
            let (api, server) = mock_pair();
            sink.lock().unwrap().push(server);
            async move {
                BidiConnection::open(&api, initial)
                    .await
                    .map_err(OpenError::new)
            }
        },
        false,
        max_topics,
        max_bytes,
    );
    (transport, servers)
}

fn take_server(servers: &Servers) -> MockServer {
    servers.lock().unwrap().remove(0)
}

#[derive(Clone, Default)]
struct WireCloseLog {
    reasons: Arc<Mutex<Vec<String>>>,
}

struct ReasonVisitor<'a>(&'a Mutex<Vec<String>>);

impl tracing::field::Visit for ReasonVisitor<'_> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "reason" {
            self.0.lock().unwrap().push(value.to_owned());
        }
    }

    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for WireCloseLog {
    fn on_record(
        &self,
        _: &tracing::Id,
        values: &tracing::span::Record<'_>,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        values.record(&mut ReasonVisitor(&self.reasons));
    }
}

async fn close_reasons(log: &WireCloseLog, n: usize) -> Vec<String> {
    let poll = xmtp_common::wait_for_some(|| async {
        let reasons = log.reasons.lock().unwrap();
        (reasons.len() >= n).then(|| reasons.clone())
    });
    match xmtp_common::time::timeout(WAIT, poll).await {
        Ok(Some(reasons)) => reasons,
        _ => panic!(
            "expected {n} closed wire spans, saw {:?}",
            log.reasons.lock().unwrap()
        ),
    }
}

fn group_topic(id: &[u8]) -> Topic {
    Topic::new_group_message(id)
}

fn welcome_topic(installation_key: &[u8]) -> Topic {
    TopicKind::WelcomeMessagesV1.create(installation_key)
}

fn group_msg(id: u64, group_id: &[u8]) -> GroupMessage {
    envelope(
        id,
        group_topic(group_id),
        backend_v1::client_envelope::Payload::GroupMessage(backend_v1::GroupMessage {
            data: vec![0xda; 4],
            ..Default::default()
        }),
    )
}

fn welcome_msg(id: u64, installation_key: &[u8]) -> WelcomeMessage {
    envelope(
        id,
        welcome_topic(installation_key),
        backend_v1::client_envelope::Payload::WelcomeMessage(backend_v1::WelcomeMessage {
            version: Some(backend_v1::welcome_message::Version::V1(
                backend_v1::welcome_message::V1 {
                    installation_key: installation_key.to_vec(),
                    data: vec![0xef; 4],
                    ..Default::default()
                },
            )),
        }),
    )
}

fn envelope(
    sequence_id: u64,
    topic: Topic,
    payload: backend_v1::client_envelope::Payload,
) -> ServerEnvelope {
    ServerEnvelope {
        meta: Some(backend_v1::EnvelopeMeta {
            topic: Some(backend_v1::Topic {
                topic: topic.cloned_vec(),
            }),
            cursor: Some(backend_v1::Cursor { sequence_id }),
            ..Default::default()
        }),
        envelope: Some(backend_v1::ClientEnvelope {
            payload: Some(payload),
        }),
    }
}

fn messages(
    group: Vec<GroupMessage>,
    welcome: Vec<WelcomeMessage>,
) -> subscribe_response::Response {
    subscribe_response::Response::Messages(subscribe_response::Messages {
        envelopes: group.into_iter().chain(welcome).collect(),
    })
}

fn started(keepalive: u32) -> subscribe_response::Response {
    subscribe_response::Response::Started(subscribe_response::Started {
        keepalive_interval_ms: keepalive,
    })
}

async fn recv(lease: &mut TopicLease<BackendBinding>) -> Option<LeaseEvent<BackendBinding>> {
    xmtp_common::time::timeout(WAIT, lease.next())
        .await
        .expect("timed out waiting for a lease event")
}

async fn wait_for_server(servers: &Servers) -> MockServer {
    let poll = xmtp_common::wait_for_some(|| async {
        let mut parked = servers.lock().unwrap();
        (!parked.is_empty()).then(|| parked.remove(0))
    });
    xmtp_common::time::timeout(WAIT, poll)
        .await
        .expect("timed out waiting for a reconnect open")
        .expect("no reconnect open")
}

fn ledger_task(
    ledger: Ledger<BackendBinding>,
    outbox: Outbox<Mutate>,
) -> LedgerTask<BackendBinding> {
    let (cmds, receiver) = mpsc::unbounded_channel();
    LedgerTask {
        opener: Box::new(
            |_| -> BoxDynFuture<'static, Result<Connection<BackendBinding>, OpenError>> {
                Box::pin(std::future::pending())
            },
        ),
        cmds: receiver,
        lease_cmds: cmds.downgrade(),
        ledger,
        conn: None,
        reconnect_delay: RECONNECT_INITIAL_DELAY,
        reconnect_at: tokio::time::Instant::now(),
        wire_opened_at: None,
        wire_span: None,
        suspended: false,
        wire_opens: 0,
        resume_notify: vec![],
        outbox,
        deferred: std::collections::VecDeque::new(),
    }
}

#[derive(Debug, thiserror::Error)]
#[error("no wire for you")]
struct Refused;

impl xmtp_common::RetryableError for Refused {
    fn is_retryable(&self) -> bool {
        false
    }
}

mod catch_up;
mod coalescing;
mod delivery;
mod limits;
mod reconnect;
mod suspend;
