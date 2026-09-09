//! Scripted backend subscription peer shared by connection and transport tests.

use futures::{StreamExt, stream::BoxStream};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use xmtp_common::time::Duration;
use xmtp_proto::api::ApiClientError;
use xmtp_proto::api_client::XmtpMlsBidiStreams;
use xmtp_proto::backend_v1::subscribe_request::Update as Mutate;
use xmtp_proto::backend_v1::{
    self, SubscribeRequest, SubscribeResponse, subscribe_request, subscribe_response,
};
use xmtp_proto::types::Topic;

const WAIT: Duration = Duration::from_secs(5);

pub(crate) struct MockApi {
    inbound: Mutex<
        Option<tokio::sync::mpsc::UnboundedReceiver<Result<SubscribeResponse, ApiClientError>>>,
    >,
    captured: tokio::sync::mpsc::UnboundedSender<SubscribeRequest>,
}

pub(crate) struct MockServer {
    pub(crate) to_client:
        tokio::sync::mpsc::UnboundedSender<Result<SubscribeResponse, ApiClientError>>,
    pub(crate) from_client: tokio::sync::mpsc::UnboundedReceiver<SubscribeRequest>,
    pub(crate) updates: HashMap<u64, Mutate>,
    active: Mutex<HashSet<Vec<u8>>>,
    last_ack: Mutex<u64>,
}

pub(crate) fn mock_pair() -> (MockApi, MockServer) {
    let (to_client, inbound) = tokio::sync::mpsc::unbounded_channel();
    let (captured, from_client) = tokio::sync::mpsc::unbounded_channel();
    (
        MockApi {
            inbound: Mutex::new(Some(inbound)),
            captured,
        },
        MockServer {
            to_client,
            from_client,
            updates: HashMap::new(),
            active: Mutex::default(),
            last_ack: Mutex::new(0),
        },
    )
}

#[xmtp_common::async_trait]
impl XmtpMlsBidiStreams for MockApi {
    type SubscribeStream = BoxStream<'static, Result<SubscribeResponse, ApiClientError>>;
    type Error = ApiClientError;

    fn host(&self) -> &str {
        "mock://bidi"
    }

    async fn subscribe_bidi(
        &self,
        requests: BoxStream<'static, SubscribeRequest>,
    ) -> Result<Self::SubscribeStream, Self::Error> {
        let captured = self.captured.clone();
        xmtp_common::spawn(None, async move {
            let mut requests = requests;
            while let Some(frame) = requests.next().await {
                let _ = captured.send(frame);
            }
        });
        let mut inbound = self
            .inbound
            .lock()
            .unwrap()
            .take()
            .expect("subscribe_bidi called twice on one mock session");
        Ok(Box::pin(futures::stream::poll_fn(move |cx| {
            inbound.poll_recv(cx)
        })))
    }
}

impl MockServer {
    pub(crate) fn send_raw(&self, response: SubscribeResponse) {
        self.to_client.send(Ok(response)).unwrap();
    }

    /// A model can finish a delivery after the client has closed its stream.
    pub(crate) fn send_if_open(&self, response: subscribe_response::Response) {
        let _ = self.to_client.send(Ok(SubscribeResponse {
            response: Some(response),
        }));
    }

    pub(crate) async fn next_request(&mut self) -> subscribe_request::Request {
        let frame = self.from_client.recv().await.expect("client closed");
        frame.request.expect("client sent empty request")
    }

    pub(crate) fn send(&self, response: subscribe_response::Response) {
        self.to_client
            .send(Ok(SubscribeResponse {
                response: Some(response),
            }))
            .unwrap();
    }

    pub(crate) fn ack(&self, id: u64, targets: Vec<(Topic, u64)>) {
        let update = self.updates.get(&id).expect("update was received");
        let mut last_ack = self.last_ack.lock().unwrap();
        assert!(id > *last_ack, "acknowledgements must follow update order");
        *last_ack = id;
        let mut active = self.active.lock().unwrap();
        for topic in &update.removes {
            active.remove(&topic.topic);
        }
        let targets: HashMap<_, _> = targets
            .into_iter()
            .map(|(topic, target)| (topic.cloned_vec(), target))
            .collect();
        let added_targets = update
            .adds
            .iter()
            .filter_map(|add| {
                let topic = add.topic.as_ref().unwrap();
                active
                    .insert(topic.topic.clone())
                    .then(|| backend_v1::CatchupTarget {
                        topic: Some(topic.clone()),
                        through_sequence_id: targets.get(&topic.topic).copied().unwrap_or(0),
                    })
            })
            .collect();
        self.send(subscribe_response::Response::Applied(
            subscribe_response::Applied { id, added_targets },
        ));
    }

    pub(crate) fn ack_empty(&self, id: u64) {
        self.ack(id, vec![]);
    }

    pub(crate) async fn next_mutate(&mut self) -> Mutate {
        let frame = xmtp_common::time::timeout(WAIT, self.from_client.recv())
            .await
            .expect("timed out waiting for a client frame")
            .expect("client closed the request stream");
        match frame.request.expect("client sent empty request") {
            subscribe_request::Request::Update(mutate) => {
                self.updates.insert(mutate.id, mutate.clone());
                mutate
            }
            other => panic!("expected a Mutate, got {other:?}"),
        }
    }

    pub(crate) async fn next_ping(&mut self) -> u64 {
        let frame = xmtp_common::time::timeout(WAIT, self.from_client.recv())
            .await
            .expect("timed out waiting for a client frame")
            .expect("client closed the request stream");
        match frame.request.expect("client sent empty request") {
            subscribe_request::Request::Ping(ping) => ping.nonce,
            other => panic!("expected a Ping, got {other:?}"),
        }
    }

    pub(crate) async fn request_stream_ended(&mut self) {
        loop {
            match xmtp_common::time::timeout(WAIT, self.from_client.recv())
                .await
                .expect("timed out waiting for the request half-close")
            {
                Some(_) => continue, // drain trailing frames (e.g. a remove wave)
                None => return,
            }
        }
    }
}
