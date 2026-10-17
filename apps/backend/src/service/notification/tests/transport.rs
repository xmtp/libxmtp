use super::*;
use crate::test_support::auth::{TestKey, mint, valid_claims};

#[xmtp_common::test(unwrap_try = true)]
async fn malformed_protobuf_has_a_constant_status_on_each_wire_method() {
    let server = TestServer::new(configured).await?;
    let url = url::Url::parse(&server.url)?;
    let socket =
        tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap())).await?;
    let (mut sender, connection) = h2::client::handshake(socket).await?;
    let connection = tokio::spawn(connection);
    for method in ["Register", "Unregister", "UpdateSubscriptions"] {
        for body in [
            vec![0, 0, 0, 0, 1, 0xff],
            vec![0, 0, 0, 0, 3, 0x0a, 0x02, 0x01],
            vec![0, 0, 0, 0, 1, 0],
            vec![],
            vec![0, 0],
            vec![0, 0, 0, 0, 2, 0],
            vec![1, 0, 0, 0, 0],
            vec![2, 0, 0, 0, 0],
        ] {
            sender = sender.ready().await?;
            let request = http::Request::builder()
                .method("POST")
                .uri(format!(
                    "{}/xmtp.backend.v1.NotificationService/{method}",
                    server.url
                ))
                .header("content-type", "application/grpc")
                .body(())?;
            let (response, mut stream) = sender.send_request(request, false)?;
            stream.send_data(bytes::Bytes::from(body), true)?;
            let response = response.await?;
            let error = Status::from_header_map(response.headers())
                .expect("decoder returns a trailers-only status");
            status(error, Code::InvalidArgument, MALFORMED_REQUEST);
        }
    }
    connection.abort();
    server.stop().await?;
}

fn authorized<T>(request: T, token: &str) -> Request<T> {
    let mut request = Request::new(request);
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
}

#[xmtp_common::test(unwrap_try = true)]
async fn database_unavailability_has_a_constant_status_on_each_method() {
    let server = TestServer::new(configured).await?;
    let mut client = server.notifications();
    client.register(registration()).await?;
    server.backend.store.primary.close().await;
    status(
        client.register(registration()).await.unwrap_err(),
        Code::Unavailable,
        "database operation failed",
    );
    status(
        client
            .update_subscriptions(update(vec![], vec![]))
            .await
            .unwrap_err(),
        Code::Unavailable,
        "database operation failed",
    );
    status(
        client.unregister(unregister()).await.unwrap_err(),
        Code::Unavailable,
        "database operation failed",
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn startup_rejects_expiry_that_cannot_fit_the_database_clock() {
    let error = TestServer::new(|config| {
        config.push.recipient_ttl_seconds = i64::MAX / xmtp_common::NS_IN_SEC;
    })
    .await
    .err()
    .expect("current database clock cannot fit the maximum duration");
    assert!(error.to_string().contains("push.recipient_ttl_seconds"));
}

#[xmtp_common::test(unwrap_try = true)]
async fn all_notification_methods_use_global_auth_and_scopes_before_identity_checks() {
    let key = TestKey::es256();
    let server = TestServer::new(|config| {
        configured(config);
        let mut auth = key.auth_config();
        auth.required_scopes = vec!["notifications".into()];
        config.auth = Some(auth);
    })
    .await?;
    let mut client = server.notifications();
    assert_eq!(
        client
            .register(api::RegisterRequest::default())
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );
    assert_eq!(
        client
            .unregister(api::UnregisterRequest::default())
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );
    assert_eq!(
        client
            .update_subscriptions(api::UpdateSubscriptionsRequest::default())
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );
    let insufficient = mint(&valid_claims(), &key);
    assert_eq!(
        client
            .register(authorized(api::RegisterRequest::default(), &insufficient))
            .await
            .unwrap_err()
            .code(),
        Code::PermissionDenied
    );
    assert_eq!(
        client
            .unregister(authorized(api::UnregisterRequest::default(), &insufficient))
            .await
            .unwrap_err()
            .code(),
        Code::PermissionDenied
    );
    assert_eq!(
        client
            .update_subscriptions(authorized(
                api::UpdateSubscriptionsRequest::default(),
                &insufficient
            ))
            .await
            .unwrap_err()
            .code(),
        Code::PermissionDenied
    );
    let mut claims = valid_claims();
    claims["scope"] = "notifications".into();
    let token = mint(&claims, &key);
    client.register(authorized(registration(), &token)).await?;
    client
        .update_subscriptions(authorized(update(vec![subscription(1, 0)], vec![]), &token))
        .await?;
    client.unregister(authorized(unregister(), &token)).await?;
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_health_is_serving_and_labels_are_bounded() {
    let server = TestServer::new(configured).await?;
    let mut health = tonic_health::pb::health_client::HealthClient::new(server.channel.clone());
    for service in [
        "QueryService",
        "PublishService",
        "SubscriptionService",
        "IdentityService",
        "NotificationService",
    ] {
        let response = health
            .check(tonic_health::pb::HealthCheckRequest {
                service: format!("xmtp.backend.v1.{service}"),
            })
            .await?
            .into_inner();
        assert_eq!(
            response.status,
            tonic_health::pb::health_check_response::ServingStatus::Serving as i32
        );
    }
    for method in ["Register", "Unregister", "UpdateSubscriptions"] {
        let labels = crate::telemetry::RpcLabels::from_path(&format!(
            "/xmtp.backend.v1.NotificationService/{method}"
        ));
        assert_eq!(labels.service, "xmtp.backend.v1.NotificationService");
        assert_eq!(labels.method, method);
    }
    assert_eq!(
        crate::telemetry::RpcLabels::from_path(
            "/xmtp.backend.v1.NotificationService/private-recipient"
        )
        .method,
        "unknown"
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
async fn failed_requests_omit_credentials_and_topics_from_logs_and_spans() {
    use crate::test_support::metrics::{isolated, value};
    let Some(metrics) = isolated(
        "service::notification::tests::transport::failed_requests_omit_credentials_and_topics_from_logs_and_spans",
    ) else {
        return;
    };
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Trace);
    tracing::dispatcher::set_global_default(capture.dispatch())?;
    let server = TestServer::new(configured).await?;
    let mut client = server.notifications();
    client.register(registration()).await?;
    client
        .update_subscriptions(update(vec![subscription(1, 1)], vec![]))
        .await?;
    let mut register = registration();
    register.recipient_secret = b"0123456789abcdef0123456789abcdef".to_vec();
    register.delivery = Some(api::register_request::Delivery::Http(api::HttpDelivery {
        url: "https://private-recipient.example/sensitive".into(),
        signing_key: b"private-webhook-signing-key".to_vec(),
    }));
    let mut change = update(vec![subscription(99, 3)], vec![]);
    change.recipient_secret = register.recipient_secret.clone();
    let secrets = [
        "0123456789abcdef0123456789abcdef".to_owned(),
        "private-recipient.example".to_owned(),
        "private-webhook-signing-key".to_owned(),
        "device-token".to_owned(),
        hex::encode(&register.recipient_id),
        format!("{:?}", register.recipient_id),
        hex::encode(&change.adds[0].topic),
        format!("{:?}", change.adds[0].topic),
        format!("{:?}", change.adds[0].hmac_keys),
    ];
    let register_error = client.register(register).await.unwrap_err();
    let update_error = client.update_subscriptions(change).await.unwrap_err();
    status(
        register_error,
        Code::PermissionDenied,
        "recipient secret is not valid",
    );
    status(
        update_error,
        Code::PermissionDenied,
        "recipient secret is not valid",
    );
    client
        .update_subscriptions(update(vec![], vec![subscription(1, 0).topic]))
        .await?;
    client.unregister(unregister()).await?;
    server.stop().await?;
    let output = capture.output();
    assert!(output.contains("gRPC request completed"));
    assert!(output.contains("recipient secret is not valid"));
    for (metric, action) in [
        ("xmtp_push_recipients_total", "registered"),
        ("xmtp_push_recipients_total", "unregistered"),
        ("xmtp_push_subscriptions_total", "added"),
        ("xmtp_push_subscriptions_total", "removed"),
    ] {
        assert_eq!(value(&metrics, metric, &[("action", action)]), 1.0);
    }
    let rendered = metrics.render();
    for secret in secrets {
        assert!(!output.contains(&secret));
        assert!(!rendered.contains(&secret));
    }
}
