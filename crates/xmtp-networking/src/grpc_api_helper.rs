use crate::gen::xmtp::message_api::v1;

use tonic::{metadata::MetadataValue, transport::Channel, Request};

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

// Subscribe to a topic and get a stream of messages, but as soon as you get on message, subscribe
// the consumer will call this method again to get the next message
pub async fn subscribe_once(
    host: String,
    topics: Vec<String>,
) -> Result<v1::Envelope, tonic::Status> {
    let mut client = v1::message_api_client::MessageApiClient::connect(host)
        .await
        .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;
    let request = v1::SubscribeRequest {
        content_topics: topics,
        ..Default::default()
    };
    let mut stream = client.subscribe(request).await?.into_inner();
    // Get the first message from the stream
    let response = stream.message().await;
    // If Option has Envelope, return it, otherwise return an error
    response
        .map(|e| e.unwrap())
        .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))
}

// Subscribe serialized version
pub async fn subscribe_once_serialized(
    host: String,
    topics: Vec<String>,
) -> Result<String, String> {
    let response = subscribe_once(host, topics)
        .await
        .map_err(|e| format!("{}", e))?;
    // Response is a v1::Envelope protobuf message, which we need to serialize to JSON
    let json = serde_json::to_string(&response).map_err(|e| format!("{}", e))?;
    Ok(json)
}

// Return the json serialization of an Envelope with bytes
pub fn test_envelope() -> String {
    let envelope = v1::Envelope {
        message: vec![65],
        ..Default::default()
    };

    serde_json::to_string(&envelope).unwrap()
}
