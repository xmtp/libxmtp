extern crate wasm_bindgen_test;
extern crate xmtp_api_grpc_gateway;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use xmtp_api_grpc_gateway::XmtpGrpcGatewayClient;
use xmtp_proto::api_client::XmtpApiClient;
use xmtp_proto::xmtp::message_api::v1::{
    BatchQueryRequest, Envelope, PublishRequest, QueryRequest,
};

// Only run these tests in a browser.
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[wasm_bindgen_test]
pub async fn test_query_publish_query() {
    let xmtp_url: String = "http://localhost:5555".to_string();
    let topic = uuid::Uuid::new_v4();
    let auth_token = ""; // TODO

    let api = XmtpGrpcGatewayClient::new(xmtp_url);
    let q = QueryRequest {
        content_topics: vec![topic.to_string()],
        ..QueryRequest::default()
    };

    // At first there's nothing there.
    let res = api.query(q.clone()).await.expect("successfully queried");
    assert_eq!(0, res.envelopes.len());

    // But after we publish something...
    api.publish(
        auth_token.to_string(),
        PublishRequest {
            envelopes: vec![Envelope {
                content_topic: topic.to_string(),
                message: vec![1, 2, 3, 4],
                timestamp_ns: 1234,
            }],
        },
    )
    .await
    .expect("published");

    // ... then we should see it in the query.
    let res = api.query(q.clone()).await.expect("successfully queried");
    assert_eq!(1, res.envelopes.len());
    assert_eq!(topic.to_string(), res.envelopes[0].content_topic);
    assert_eq!(1234, res.envelopes[0].timestamp_ns);
    assert_eq!(vec![1, 2, 3, 4], res.envelopes[0].message);
}

#[wasm_bindgen_test]
pub async fn test_batch_query_publish_batch_query() {
    let xmtp_url: String = "http://localhost:5555".to_string();
    let api = XmtpGrpcGatewayClient::new(xmtp_url);
    let topic1 = uuid::Uuid::new_v4();
    let topic2 = uuid::Uuid::new_v4();
    let auth_token = ""; // TODO

    // First we issue this batch query and get no results.
    let batch_q = BatchQueryRequest {
        requests: vec![
            QueryRequest {
                content_topics: vec![topic1.to_string()],
                ..QueryRequest::default()
            },
            QueryRequest {
                content_topics: vec![topic2.to_string()],
                ..QueryRequest::default()
            },
        ],
    };
    let res = api
        .batch_query(batch_q.clone())
        .await
        .expect("successfully batch queried");
    assert_eq!(2, res.responses.len());
    assert_eq!(0, res.responses[0].envelopes.len());
    assert_eq!(0, res.responses[1].envelopes.len());

    // Now we publish to both of the topics...
    api.publish(
        auth_token.to_string(),
        PublishRequest {
            envelopes: vec![
                Envelope {
                    content_topic: topic1.to_string(),
                    message: vec![1, 1, 1, 1],
                    timestamp_ns: 1111,
                },
                Envelope {
                    content_topic: topic2.to_string(),
                    message: vec![2, 2, 2, 2],
                    timestamp_ns: 2222,
                },
            ],
        },
    )
    .await
    .expect("published to both of them");

    // ... so when we batch query again we should see the results.
    let res = api
        .batch_query(batch_q.clone())
        .await
        .expect("successfully batch queried again");
    assert_eq!(2, res.responses.len());
    assert_eq!(1, res.responses[0].envelopes.len());
    assert_eq!(1, res.responses[1].envelopes.len());
    let e1: Envelope;
    let e2: Envelope;
    if res.responses[0].envelopes[0].content_topic == topic1.to_string() {
        e1 = res.responses[0].envelopes[0].clone();
        e2 = res.responses[1].envelopes[0].clone();
    } else {
        e1 = res.responses[1].envelopes[0].clone();
        e2 = res.responses[0].envelopes[0].clone();
    }
    assert_eq!(1111, e1.timestamp_ns);
    assert_eq!(2222, e2.timestamp_ns);
    assert_eq!(vec![1, 1, 1, 1], e1.message);
    assert_eq!(vec![2, 2, 2, 2], e2.message);
}
