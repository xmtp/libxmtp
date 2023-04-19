use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;
use tonic::{metadata::MetadataValue, transport::Channel, Request, Streaming};
use xmtp_proto::xmtp::message_api::v1::{self, Envelope};

// Do a barebones unpaginated Query gRPC request
// With optional PagingInfo
pub async fn query(
    host: String,
    topic: String,
    paging_info: Option<v1::PagingInfo>,
) -> Result<v1::QueryResponse, tonic::Status> {
    let mut client = v1::message_api_client::MessageApiClient::connect(host)
        .await
        .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;
    let mut request = v1::QueryRequest {
        content_topics: vec![topic],
        ..Default::default()
    };
    // Check if paging_info is not None
    if let Some(p) = paging_info {
        request.paging_info = Some(p);
    }
    // Do the query and get a Tonic response that we need to process
    let response = client.query(request).await;
    response.map(|r| r.into_inner())
}

// Do a JSON serialized version of query, where the v1::QueryResponse is JSON serialized
pub async fn query_serialized(
    host: String,
    topic: String,
    json_paging_info: String,
) -> Result<String, String> {
    // Check if json_paging_info is not an empty string, if so deserialize it
    let paging_info = if !json_paging_info.is_empty() {
        let p: v1::PagingInfo =
            serde_json::from_str(&json_paging_info).map_err(|e| format!("{}", e))?;
        Some(p)
    } else {
        None
    };

    // Do the query and get a Tonic response that we need to process
    let response = query(host, topic, paging_info)
        .await
        .map_err(|e| format!("{}", e))?;
    // Response is a v1::QueryResponse protobuf message, which we need to serialize to JSON
    let json = serde_json::to_string(&response).map_err(|e| format!("{}", e))?;
    Ok(json)
}

// Publish a message to the XMTP server at a topic with some string content
pub async fn publish(
    host: String,
    token: String,
    json_envelopes: String,
) -> Result<v1::PublishResponse, tonic::Status> {
    let host = host.to_string();
    let channel = Channel::from_shared(host)
        .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?
        .connect()
        .await
        .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;

    let auth_token_string = format!("Bearer {}", token);
    let token: MetadataValue<_> = auth_token_string
        .parse()
        .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;

    let mut client = v1::message_api_client::MessageApiClient::with_interceptor(
        channel,
        move |mut req: Request<()>| {
            req.metadata_mut().insert("authorization", token.clone());
            Ok(req)
        },
    );

    let mut request = v1::PublishRequest::default();
    // Deserialize the JSON string into a vector of Envelopes
    let envelopes: Vec<v1::Envelope> = serde_json::from_str(&json_envelopes)
        .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;
    request.envelopes = envelopes;
    let response = client.publish(request).await;
    response.map(|r| r.into_inner())
}

// Serialized version of publish
pub async fn publish_serialized(
    host: String,
    token: String,
    json_envelopes: String,
) -> Result<String, String> {
    let response = publish(host, token, json_envelopes)
        .await
        .map_err(|e| format!("{}", e))?;
    // Response is a v1::PublishResponse protobuf message, which we need to serialize to JSON
    let json = serde_json::to_string(&response).map_err(|e| format!("{}", e))?;
    Ok(json)
}

pub async fn subscribe(host: String, topics: Vec<String>) -> Result<Subscription, tonic::Status> {
    let mut client = v1::message_api_client::MessageApiClient::connect(host)
        .await
        .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;
    let request = v1::SubscribeRequest {
        content_topics: topics,
        ..Default::default()
    };
    let stream = client.subscribe(request).await.unwrap().into_inner();

    return Ok(Subscription::start(stream).await);
}

// Return the json serialization of an Envelope with bytes
pub fn test_envelope(topic: String) -> String {
    let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let envelope = v1::Envelope {
        timestamp_ns: time_since_epoch.as_nanos() as u64,
        content_topic: topic.to_string(),
        message: vec![65],
        ..Default::default()
    };
    serde_json::to_string(&vec![envelope]).unwrap()
}

pub struct Subscription {
    pending: Arc<Mutex<Vec<Envelope>>>,
    close_tx: Option<oneshot::Sender<()>>,
}

impl Subscription {
    pub async fn start(stream: Streaming<Envelope>) -> Self {
        let pending = Arc::new(Mutex::new(Vec::new()));
        let pending_clone = pending.clone();
        let (close_tx, close_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let mut stream = Box::pin(stream);
            let mut close_rx = Box::pin(close_rx);

            loop {
                tokio::select! {
                    item = stream.message() => {
                        match item {
                            Ok(Some(envelope)) => {
                                let mut pending = pending_clone.lock().unwrap();
                                pending.push(envelope);
                            }
                            _ => break,
                        }
                    },
                    _ = &mut close_rx => {
                        break;
                    }
                }
            }
        });

        Subscription {
            pending,
            close_tx: Some(close_tx),
        }
    }

    pub fn get_and_reset_pending(&self) -> Vec<Envelope> {
        let mut pending = self.pending.lock().unwrap();
        let items = pending.drain(..).collect::<Vec<Envelope>>();
        items
    }

    pub fn close_stream(&mut self) {
        if let Some(close_tx) = self.close_tx.take() {
            let _ = close_tx.send(());
        }
    }
}
