use std::time::Duration;

use crate::XmtpHttpApiClient;
use futures::StreamExt;
use xmtp_proto::{client_traits::Client, mls_v1::GroupMessage};

use super::mock::*;
use httpmock::prelude::*;
use prost::Message;

fn apply_messages(mut then: httpmock::Then) -> httpmock::Then {
    let messages = generate_messages(3)
        .into_iter()
        .map(|m| m.encode_to_vec())
        .collect::<Vec<_>>()
        .join(&b'\n');
    then.body(messages)
}

#[tokio::test]
async fn test_bytes_stream() {
    // Start a lightweight mock server.
    let server = MockServer::start_async().await;

    // Create a mock on the server.
    let hello_mock = server
        .mock_async(|when, then| {
            when.method("POST").path("/subscribe");
            then.status(200)
                .delay(Duration::from_secs(3))
                .and(apply_messages);
        })
        .await;

    let client = XmtpHttpApiClient::new(server.base_url(), "0.0.0".into())
        .await
        .unwrap();
    let request = http::Request::builder();
    let path = http::uri::PathAndQuery::try_from("/subscribe").unwrap();
    let s = client
        .stream(request, path, Default::default())
        .await
        .unwrap()
        .into_body();
    futures::pin_mut!(s);
    let one = s.next().await.unwrap().unwrap();
    tracing::info!("{}", String::from_utf8_lossy(&one));
    tracing::info!("{:?}", GroupMessage::decode(one).unwrap());
    let one = s.next().await.unwrap().unwrap();
    tracing::info!("{:?}", GroupMessage::decode(one).unwrap());
    let one = s.next().await.unwrap().unwrap();
    tracing::info!("{:?}", GroupMessage::decode(one).unwrap());
    // Send an HTTP request to the mock server. This simulates your code.
    // let client = reqwest::Client::new();
    // let response = client.get(server.url("/translate?word=hello")).send().await.unwrap();

    // Ensure the specified mock was called exactly one time (or fail with a
    // detailed error description).
    hello_mock.assert();

    // Ensure the mock server did respond as specified.
    // assert_eq!(response.status(), 200);
}
