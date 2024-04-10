use wasm_bindgen_test::*;
use xmtp_api_grpc::auth_token::Authenticator;
use xmtp_api_grpc_gateway::XmtpGrpcGatewayClient;
use xmtp_proto::api_client::XmtpApiClient;
use xmtp_proto::xmtp::message_api::v1::{
    BatchQueryRequest, Envelope, PublishRequest, QueryRequest,
};

// Only run these tests in a browser.
wasm_bindgen_test_configure!(run_in_browser);

const PRIVATE_KEY_BUNDLE_HEX: &str = "0a88030ac20108eec0888ae33112220a201cd19d1d6e129cb8f8ba4bd85aae10ffcc97a3de939d85f9bc378d47e6ba83711a940108eec0888ae33112460a440a40130cfb1cd667f48585f90372fe4b529da318e83221a3bfd1446ef6cf00d173543fed831d1517d310b05bd5ab138fde22af50a3ffce1aa72da8c7084e9bab0e4910011a430a4104c4eb77c3b2eaacaca12e2b55c6c42dc33f4518a5690bb49cd6ae0e0a652e59fbc9defd98242d30a0737a13c3461cac1edc0f8e3007d65b1637382088ac1cd3d712c00108a4c1888ae33112220a2062e553bceac5247e7bebfdcc8c31959965603e442f79c6346028060ab2129e931a920108a4c1888ae33112440a420a40d12c6ab6eb1874edd3044fdc753543516130bd4d1db11024bd81cd9c2c4bb6b6138e85ed313f387ea7707e09090659b580ee22f42f022c4521e4a11ab7abddfc1a430a4104175097c31bbe1700729f1f1ede87b8bd21a5bc62e4bb4c963e0de885080048bd31138b657fd9146aa8255f1c57c4fa1f8cb7b30bed8803eed48d6a3e67e71ccf";
const WALLET_ADDRESS: &str = "0xA38A1f04B29dea1de621E17447fB4efB11BFfBdf";

fn get_auth_token() -> String {
    // This is a private key bundle exported from the JS SDK and hex encoded
    let authenticator = Authenticator::from_hex(
        PRIVATE_KEY_BUNDLE_HEX.to_string(),
        WALLET_ADDRESS.to_string(),
    );
    authenticator.create_token()
}

#[wasm_bindgen_test]
pub async fn test_query_publish_query() {
    let xmtp_url: String = "http://localhost:5555".to_string();
    let topic = uuid::Uuid::new_v4();
    let auth_token = get_auth_token();

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
        auth_token,
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
    let auth_token = get_auth_token();

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
