use crate::api::{
    self, subscribe_request::Request as Input, subscribe_response::Response as Frame,
};
use crate::test_support::{RunningServer, TestResult};
use bytes::Bytes;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Status, Streaming};
use xmtp_mls_validation::test_utils::inline_welcome_envelope;

pub struct Native {
    pub input: mpsc::Sender<api::SubscribeRequest>,
    pub output: Streaming<api::SubscribeResponse>,
}
impl Native {
    pub async fn open(server: &RunningServer) -> TestResult<Self> {
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

/// `message` with its gRPC prefix: flag 0 and the big-endian length.
pub fn framed(message: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0];
    bytes.extend_from_slice(&(message.len() as u32).to_be_bytes());
    bytes.extend_from_slice(message);
    bytes
}

/// Send `body` as the raw HTTP/2 body of one gRPC request with `headers`
/// added, and return the call's status, from a trailers-only response or from
/// the trailers. With `end_stream` false the request body stays open while
/// the response is awaited. Waits at most five seconds for the response.
pub async fn raw_unary(
    server: &RunningServer,
    path: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
    end_stream: bool,
) -> TestResult<Status> {
    let url = url::Url::parse(&server.url)?;
    let socket = tokio::net::TcpStream::connect((
        url.host_str().ok_or("no host")?,
        url.port().ok_or("no port")?,
    ))
    .await?;
    let (sender, connection) = h2::client::handshake(socket).await?;
    let connection = tokio::spawn(connection);
    let mut request = http::Request::builder()
        .method("POST")
        .uri(format!("{}{path}", server.url))
        .header("content-type", "application/grpc");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let request = request.body(())?;
    let mut sender = sender.ready().await?;
    let (response, mut stream) = sender.send_request(request, false)?;
    stream.send_data(Bytes::from(body), end_stream)?;
    let status = xmtp_common::time::timeout(xmtp_common::time::Duration::from_secs(5), async {
        let response = response.await?;
        if let Some(status) = Status::from_header_map(response.headers()) {
            return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(status);
        }
        let mut body = response.into_body();
        while let Some(data) = body.data().await {
            let _ = body.flow_control().release_capacity(data?.len());
        }
        let trailers = body.trailers().await?.ok_or("no trailers")?;
        Ok(Status::from_header_map(&trailers).ok_or("no grpc-status")?)
    })
    .await??;
    drop((stream, sender));
    connection.abort();
    Ok(status)
}
