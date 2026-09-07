use crate::api::{
    self, subscribe_request::Request as Input, subscribe_response::Response as Frame,
};
use crate::test_support::{TestResult, TestServer};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Status, Streaming};
use xmtp_mls_validation::test_utils::inline_welcome_envelope;

pub struct Native {
    pub input: mpsc::Sender<api::SubscribeRequest>,
    pub output: Streaming<api::SubscribeResponse>,
}
impl Native {
    pub async fn open(server: &TestServer) -> TestResult<Self> {
        let (input, receiver) = mpsc::channel(128);
        let output = api::subscription_service_client::SubscriptionServiceClient::new(
            server.channel.clone(),
        )
        .subscribe(ReceiverStream::new(receiver))
        .await?
        .into_inner();
        let mut stream = Self { input, output };
        assert!(matches!(stream.next().await?, Frame::Started(_)));
        Ok(stream)
    }
    pub async fn send(&self, request: Input) -> TestResult {
        self.input
            .send(api::SubscribeRequest {
                request: Some(request),
            })
            .await?;
        Ok(())
    }
    pub async fn next(&mut self) -> TestResult<Frame> {
        Ok(xmtp_common::time::timeout(
            xmtp_common::time::Duration::from_secs(5),
            self.output.message(),
        )
        .await??
        .ok_or("stream ended")?
        .response
        .ok_or("empty response")?)
    }
    pub async fn update(
        &self,
        id: u64,
        adds: Vec<api::TopicQuery>,
        removes: Vec<api::Topic>,
    ) -> TestResult {
        self.send(Input::Update(api::subscribe_request::Update {
            id,
            adds,
            removes,
        }))
        .await
    }
    pub async fn messages(&mut self, count: usize) -> TestResult<Vec<api::ServerEnvelope>> {
        let mut rows = Vec::new();
        while rows.len() < count {
            match self.next().await? {
                Frame::Messages(messages) => rows.extend(messages.envelopes),
                Frame::Ping(ping) => {
                    self.send(Input::Pong(api::Pong { nonce: ping.nonce }))
                        .await?
                }
                frame => return Err(format!("unexpected frame: {frame:?}").into()),
            }
        }
        Ok(rows)
    }
}

/// Wait for one terminal result under a total deadline, even when unexpected
/// data continues to arrive. Return None only for a successful end of stream.
pub async fn terminal(
    output: &mut Streaming<api::SubscribeResponse>,
) -> TestResult<Option<Status>> {
    Ok(
        xmtp_common::time::timeout(xmtp_common::time::Duration::from_secs(5), async {
            loop {
                match output.message().await {
                    Ok(Some(_)) => {}
                    Ok(None) => return None,
                    Err(error) => return Some(error),
                }
            }
        })
        .await?,
    )
}

pub fn envelope(topic: u8, value: u8) -> api::ClientEnvelope {
    let mut envelope = inline_welcome_envelope([topic; 32]);
    if let Some(api::client_envelope::Payload::WelcomeMessage(welcome)) = &mut envelope.payload
        && let Some(api::welcome_message::Version::V1(welcome)) = &mut welcome.version
    {
        welcome.data.push(value);
    }
    envelope
}
