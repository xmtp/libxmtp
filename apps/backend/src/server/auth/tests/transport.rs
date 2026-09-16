use super::*;
use crate::{
    api,
    test_support::{RunningServer, TestServer, grpc_web},
};
use tonic_health::pb::{
    HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
};

#[xmtp_common::test(unwrap_try = true)]
async fn raw_http2_cannot_bypass_auth_with_a_non_grpc_content_type_or_unknown_path() {
    let key = TestKey::es256();
    let server = TestServer::new(|config| config.auth = Some(key.auth_config())).await?;
    let socket =
        tokio::net::TcpStream::connect(server.url.strip_prefix("http://").unwrap()).await?;
    let (mut client, connection) = h2::client::handshake(socket).await?;
    let connection = tokio::spawn(connection);
    for path in [
        "QueryService/Query",
        "QueryService/QueryNewest",
        "PublishService/Publish",
        "SubscriptionService/Subscribe",
        "SubscriptionService/SubscribeStatic",
        "IdentityService/GetInboxIds",
        "IdentityService/VerifySmartContractWalletSignatures",
        "UnknownService/Unknown",
    ] {
        let request = http::Request::post(format!("{}/xmtp.backend.v1.{path}", server.url))
            .header("content-type", "text/plain")
            .body(())?;
        let (response, mut body) = client.send_request(request, false)?;
        body.send_data(bytes::Bytes::from_static(&[0, 0, 0, 0, 0]), true)?;
        let response = response.await?;
        assert_eq!(response.headers()["grpc-status"], "16", "{path}");
        assert_eq!(
            tonic::Status::from_header_map(response.headers())
                .unwrap()
                .message(),
            "missing bearer token"
        );
    }
    drop(client);
    connection.abort();
    let _ = connection.await;
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn health_and_cors_preflight_remain_public_with_auth_enabled() {
    for auth in [TestKey::es256().auth_config(), api_key("operator").1] {
        let server = TestServer::new(|config| config.auth = Some(auth)).await?;
        let health = HealthClient::new(server.channel.clone())
            .check(HealthCheckRequest {
                service: String::new(),
            })
            .await?
            .into_inner();
        assert_eq!(health.status(), ServingStatus::Serving);
        let response = xmtp_common::http::client()?
            .request(
                reqwest::Method::OPTIONS,
                format!("{}/xmtp.backend.v1.QueryService/Query", server.url),
            )
            .header("origin", "https://app.example")
            .header("access-control-request-method", "POST")
            .header(
                "access-control-request-headers",
                "authorization,content-type",
            )
            .send()
            .await?;
        assert!(response.status().is_success());
        assert!(!response.headers().contains_key("grpc-status"));
        assert_eq!(
            response.headers()["access-control-allow-headers"],
            "authorization,content-type"
        );
        server.stop().await?;
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn authenticated_native_and_web_responses_match_auth_disabled_service() {
    let server = TestServer::new(|_| {}).await?;
    let envelope = xmtp_mls_validation::test_utils::inline_welcome_envelope([73; 32]);
    let meta = server.publish(vec![envelope]).await?.remove(0);
    let sequence_id = meta.cursor.as_ref().unwrap().sequence_id;
    let topic = meta.topic.clone().unwrap();
    // The Get RPC was removed upstream; Query is the unary equivalent. A cursor
    // one before the published sequence returns exactly that envelope.
    let query = move || api::QueryRequest {
        queries: vec![crate::test_support::query_topic(
            topic.clone(),
            sequence_id - 1,
        )],
        limit: 1,
    };
    let baseline = server.query().query(query()).await?.into_inner();
    let key = TestKey::eddsa();
    let mut config = (*server.backend.config).clone();
    config.auth = Some(key.auth_config());
    let mut protected = RunningServer::new(config).await?;
    assert_eq!(
        protected.query().query(query()).await.unwrap_err().code(),
        tonic::Code::Unauthenticated
    );
    let token = format!("Bearer {}", mint(&valid_claims(), &key));
    let mut request = tonic::Request::new(query());
    request
        .metadata_mut()
        .insert("authorization", token.parse()?);
    assert_eq!(
        protected.query().query(request).await?.into_inner(),
        baseline
    );
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("authorization", token.parse()?);
    let client = xmtp_common::http::client_builder()
        .default_headers(headers)
        .build()?;
    let mut web: grpc_web::WebStream<api::QueryResponse> = grpc_web::open(
        &client,
        &protected.url,
        "/xmtp.backend.v1.QueryService/Query",
        query(),
    )
    .await?;
    assert_eq!(web.stream.message().await?.unwrap(), baseline);
    assert!(web.stream.message().await?.is_none());
    drop(web);
    protected.stop().await?;
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn configured_keys_only_serve_without_jwt_keys() {
    let (value, auth) = api_key("operator");
    let server = TestServer::new(|_| {}).await?;
    let mut config = (*server.backend.config).clone();
    config.auth = Some(auth);
    let mut protected = RunningServer::new(config).await?;
    let mut request = tonic::Request::new(api::QueryRequest::default());
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {value}").parse()?);
    protected.query().query(request).await?;
    let mut request = tonic::Request::new(api::QueryRequest::default());
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", mint(&valid_claims(), &TestKey::es256())).parse()?,
    );
    let status = protected.query().query(request).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
    assert_eq!(status.message(), crate::auth::verify::UNTRUSTED);
    protected.stop().await?;
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn api_keys_and_jwts_both_admit_unary_requests() {
    let (value, mut auth) = api_key("operator");
    let key = TestKey::es256();
    auth.keys = Some(vec![key.config()]);
    let server = TestServer::new(|config| config.auth = Some(auth)).await?;
    for token in [value, mint(&valid_claims(), &key)] {
        let mut request = tonic::Request::new(api::QueryRequest::default());
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {token}").parse()?);
        server.query().query(request).await?;
    }
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn configured_auth_cannot_serve_without_initialized_keys() {
    let server = TestServer::new(|_| {}).await?;
    let mut backend = server.backend.clone();
    let mut config = (*backend.config).clone();
    config.auth = Some(TestKey::es256().auth_config());
    backend.config = Arc::new(config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let result = crate::server::serve(backend, listener, std::future::pending()).await;
    assert!(matches!(
        result,
        Err(crate::server::ServeError::AuthNotInitialized)
    ));
    server.stop().await?;
}
