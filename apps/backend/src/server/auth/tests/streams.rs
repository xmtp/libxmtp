use super::*;
use crate::{
    api,
    test_support::{self, TestServer, grpc_web},
};
use tokio_stream::wrappers::ReceiverStream;
use xmtp_common::time::{Duration, timeout};

fn authorized<T>(body: T, token: &str) -> tonic::Request<T> {
    let mut request = tonic::Request::new(body);
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
}

#[xmtp_common::test(unwrap_try = true)]
async fn native_and_web_subscription_opens_require_the_configured_scopes() {
    let key = TestKey::es256();
    let mut auth = key.auth_config();
    auth.required_scopes = vec!["xmtp".into(), "subscribe".into()];
    let server = TestServer::new(|config| config.auth = Some(auth)).await?;
    let mut client =
        api::subscription_service_client::SubscriptionServiceClient::new(server.channel.clone());
    assert_eq!(
        client
            .subscribe(futures::stream::pending::<api::SubscribeRequest>())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unauthenticated
    );
    assert_eq!(
        client
            .subscribe_static(api::SubscribeStaticRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unauthenticated
    );
    let mut claims = valid_claims();
    claims["scope"] = serde_json::json!("xmtp");
    let token = mint(&claims, &key);
    assert_eq!(
        client
            .subscribe(authorized(
                futures::stream::pending::<api::SubscribeRequest>(),
                &token
            ))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
    assert_eq!(
        client
            .subscribe_static(authorized(api::SubscribeStaticRequest::default(), &token))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
    for (authorization, expected) in [
        (None, tonic::Code::Unauthenticated),
        (
            Some(format!("Bearer {token}")),
            tonic::Code::PermissionDenied,
        ),
    ] {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(token) = authorization {
            headers.insert("authorization", token.parse()?);
        }
        let client = xmtp_common::http::client_builder()
            .default_headers(headers)
            .build()?;
        for method in ["Subscribe", "SubscribeStatic"] {
            // Empty protobuf messages encode identically. Auth rejects before decoding.
            let result: test_support::TestResult<grpc_web::WebStream<api::SubscribeResponse>> =
                grpc_web::open(
                    &client,
                    &server.url,
                    &format!("/xmtp.backend.v1.SubscriptionService/{method}"),
                    api::SubscribeRequest::default(),
                )
                .await;
            let error = result.err().expect("subscription must be rejected");
            assert_eq!(
                error.downcast_ref::<tonic::Status>().unwrap().code(),
                expected
            );
        }
    }
    claims["scope"] = serde_json::json!("xmtp subscribe");
    let token = mint(&claims, &key);
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse()?);
    let client = xmtp_common::http::client_builder()
        .default_headers(headers)
        .build()?;
    let mut web: grpc_web::WebStream<api::SubscribeResponse> = grpc_web::open(
        &client,
        &server.url,
        "/xmtp.backend.v1.SubscriptionService/Subscribe",
        api::SubscribeRequest {
            request: Some(api::subscribe_request::Request::Ping(api::Ping {
                nonce: 1,
            })),
        },
    )
    .await?;
    assert!(matches!(
        web.stream.message().await?.unwrap().response,
        Some(api::subscribe_response::Response::Started(_))
    ));
    drop(web);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn open_streams_continue_after_token_expiry_but_new_opens_fail() {
    let key = TestKey::es256();
    let mut auth = key.auth_config();
    auth.leeway_seconds = 0;
    let server = TestServer::new(|config| {
        config.auth = Some(auth);
        config.streams.poll_interval_ms = 10;
    })
    .await?;
    let exp = xmtp_common::time::now_secs() + 3;
    let token = mint(&serde_json::json!({"exp": exp}), &key);
    let topic = test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[74; 32]);
    let query = test_support::query_topic(topic, 0);
    let mut client =
        api::subscription_service_client::SubscriptionServiceClient::new(server.channel.clone());
    let (input, receiver) = tokio::sync::mpsc::channel(8);
    let mut bidi = client
        .subscribe(authorized(ReceiverStream::new(receiver), &token))
        .await?
        .into_inner();
    assert!(matches!(
        bidi.message().await?.unwrap().response,
        Some(api::subscribe_response::Response::Started(_))
    ));
    input
        .send(api::SubscribeRequest {
            request: Some(api::subscribe_request::Request::Update(
                api::subscribe_request::Update {
                    id: 1,
                    adds: vec![query.clone()],
                    removes: vec![],
                },
            )),
        })
        .await?;
    assert!(matches!(
        bidi.message().await?.unwrap().response,
        Some(api::subscribe_response::Response::Applied(_))
    ));
    let mut native_static = client
        .subscribe_static(authorized(
            api::SubscribeStaticRequest {
                topics: vec![query.clone()],
            },
            &token,
        ))
        .await?
        .into_inner();
    assert!(matches!(
        native_static.message().await?.unwrap().response,
        Some(api::subscribe_static_response::Response::Started(_))
    ));
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse()?);
    let web_client = xmtp_common::http::client_builder()
        .default_headers(headers)
        .build()?;
    let mut web: grpc_web::WebStream<api::SubscribeStaticResponse> = grpc_web::open(
        &web_client,
        &server.url,
        "/xmtp.backend.v1.SubscriptionService/SubscribeStatic",
        api::SubscribeStaticRequest {
            topics: vec![query.clone()],
        },
    )
    .await?;
    assert!(matches!(
        web.stream.message().await?.unwrap().response,
        Some(api::subscribe_static_response::Response::Started(_))
    ));
    xmtp_common::time::sleep(Duration::from_secs(
        (exp + 1 - xmtp_common::time::now_secs()).max(0) as u64,
    ))
    .await;
    assert!(xmtp_common::time::now_secs() > exp);
    let publish = authorized(
        api::PublishRequest {
            envelopes: vec![test_support::native::envelope(74, 1)],
        },
        &mint(&valid_claims(), &key),
    );
    let meta = server
        .publisher()
        .publish(publish)
        .await?
        .into_inner()
        .envelope_metas
        .remove(0);
    let frame = timeout(Duration::from_secs(5), bidi.message())
        .await??
        .unwrap();
    assert!(
        matches!(frame.response, Some(api::subscribe_response::Response::Messages(messages)) if messages.envelopes[0].meta == Some(meta.clone()))
    );
    for stream in [&mut native_static, &mut web.stream] {
        let frame = timeout(Duration::from_secs(5), stream.message())
            .await??
            .unwrap();
        assert!(
            matches!(frame.response, Some(api::subscribe_static_response::Response::Messages(messages)) if messages.envelopes[0].meta == Some(meta.clone()))
        );
    }
    assert_eq!(
        client
            .subscribe(authorized(
                futures::stream::pending::<api::SubscribeRequest>(),
                &token
            ))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unauthenticated
    );
    assert_eq!(
        client
            .subscribe_static(authorized(
                api::SubscribeStaticRequest {
                    topics: vec![query]
                },
                &token
            ))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unauthenticated
    );
    drop((input, bidi, native_static, web));
    server.stop().await?;
}
