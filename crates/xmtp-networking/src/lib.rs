pub mod proto_helper;
use crate::proto_helper::xmtp::message_api::v1;

use tonic::{metadata::MetadataValue, transport::Channel, Request};

pub fn test_request() -> Result<u16, String> {
    let resp = reqwest::blocking::get("https://httpbin.org/ip").map_err(|e| format!("{}", e))?;
    // if resp is successful, return the body otherwise return "Error: {}" with response code
    if resp.status().is_success() {
        Ok(resp.status().as_u16())
    } else {
        Err(format!("{}", resp.status()))
    }
}

pub fn selftest() -> u16 {
    let resp = test_request();
    resp.unwrap_or(777)
}

pub async fn test_grpc() -> bool {
    let mut client =
        proto_helper::xmtp::message_api::v1::message_api_client::MessageApiClient::connect(
            "http://localhost:5556",
        )
        .await
        .unwrap();
    // TODO: Return true if client was able to connect
    true
}

// Do a barebones unpaginated Query gRPC request, similar to this Swift code:
pub async fn query(topic: String) -> Result<v1::QueryResponse, tonic::Status> {
    // NOTE: had to edit e2e/docker compose to map port 15555->5556 instead of 5555
    let mut client =
        proto_helper::xmtp::message_api::v1::message_api_client::MessageApiClient::connect(
            "https://localhost:15555",
        )
        .await
        .unwrap();

    let mut request = proto_helper::xmtp::message_api::v1::QueryRequest::default();
    request.content_topics = vec![topic];
    // Do the query and get a Tonic response that we need to process
    let response = client.query(request).await;
    response.map(|r| r.into_inner())
}

// Publish a message to the XMTP server at a topic with some string content
pub async fn publish(topic: String, content: String) -> Result<v1::PublishResponse, tonic::Status> {
    // NOTE: had to edit e2e/docker compose to map port 15555->5556 instead of 5555
    let channel = Channel::from_static("https://localhost:15555")
        .connect()
        .await
        .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;
    // TODO: replace hardcoded token
    let token: MetadataValue<_> = "Bearer CpIBCIKUi4X4MBJECkIKQHAB57G9n+afftmrFy0S2avtyh2VNKUPPTn8n1rlUtYiTnBkwGlYgb2CMaG7KTE56qAfcnkWYC/XbWxl2CM61kYaQwpBBOvn8X5EepteFT6E1BXMLi/zhgUl+TV7GLJo/kAcEYhXEIbw//nciuv6f6R2y77sHLJmQssTT2PEG/lBgk640w0SNgoqMHgyZDM4MEQ4QUY0NmQ4MEM5YjE4MkExOWYzOWZDNjIwMTQ5NDBGQjVmEIC0o8m/0vaqFxpGCkQKQDrJyRW9avQxCdrP804eygA9rsWp7HxeYkhjcg7DF8NiFI1eJnEWk0dOUqkSGtwyV8Afmu4ckqA8vy5YwHQCudgQAQ==".parse().map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;

    let mut client =
        proto_helper::xmtp::message_api::v1::message_api_client::MessageApiClient::with_interceptor(
            channel,
            move |mut req: Request<()>| {
                req.metadata_mut().insert("authorization", token.clone());
                Ok(req)
            },
        );

    let mut request = proto_helper::xmtp::message_api::v1::PublishRequest::default();
    let mut envelope = proto_helper::xmtp::message_api::v1::Envelope::default();
    envelope.message = content.into_bytes();
    envelope.content_topic = topic;
    request.envelopes = vec![envelope];
    let response = client.publish(request).await;
    response.map(|r| r.into_inner())
}

// Blocking roundtrip test, returns an error code (0) for pass, non-zero for fail
pub fn grpc_roundtrip() -> u16 {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let resp = publish("topic".to_string(), "test".to_string()).await;
        println!("{:?}", resp);
        if resp.is_err() {
            return 1;
        }
        // Fetch it
        let query_resp = query("topic".to_string()).await;
        println!("{:?}", query_resp);
        if query_resp.is_err() {
            return 2;
        }
        // Check that the response has some messages, and that the content inside is "test"
        let envelopes = query_resp.unwrap().envelopes;
        if envelopes.len() != 1 {
            return 3;
        }
        let topic = envelopes[0].content_topic.clone();
        if topic != "topic" {
            return 4;
        }
        let content = String::from_utf8(envelopes[0].message.clone()).unwrap();
        if content != "test" {
            return 5;
        }
        0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_works() {
        let resp = selftest();
        // Assert 200
        assert_eq!(resp, 200);
    }

    #[test]
    fn grpc_query_test() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let resp = query("test".to_string()).await;
            println!("{:?}", resp);
            assert!(resp.is_ok());
            // Check that the response has some messages
            assert!(resp.unwrap().envelopes.len() == 0);
        });
    }

    #[test]
    fn grpc_roundtrip_test() {
        let resp = grpc_roundtrip();
        // Assert 0
        assert_eq!(resp, 0);
    }
}
