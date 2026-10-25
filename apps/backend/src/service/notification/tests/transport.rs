use super::*;
use crate::test_support::{
    auth::{TestKey, mint, valid_claims},
    native::{framed, raw_unary},
};
use prost::Message;
use xmtp_configuration::{NOTIFICATION_BATCH_HMAC_KEYS, NOTIFICATION_BATCH_TOPICS};

const UPDATE: &str = "/xmtp.backend.v1.NotificationService/UpdateSubscriptions";
const BATCH_LIMIT: &str = "request exceeds the subscription batch limit";

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

/// `count` distinct group-message topics.
fn topics(count: usize) -> impl Iterator<Item = Vec<u8>> {
    (0..count).map(|n| {
        let mut id = [0; 32];
        id[..4].copy_from_slice(&(n as u32).to_be_bytes());
        TopicKind::GroupMessagesV1.create(id).to_vec()
    })
}

/// A batch over the bound fails before any insert. A sequence counts insert
/// attempts, which a rollback does not undo.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PUSH-264
async fn subscription_batch_over_bound_does_no_database_work() {
    let server = TestServer::new(configured).await?;
    let mut client = server.notifications();
    client.register(registration()).await?;
    sqlx::raw_sql(
        r#"
        CREATE SEQUENCE attempted_subscription_inserts;
        CREATE FUNCTION count_subscription_insert() RETURNS trigger AS $$
        BEGIN
            PERFORM nextval('attempted_subscription_inserts');
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
        CREATE TRIGGER count_subscription_insert BEFORE INSERT ON push_subscription
        FOR EACH ROW EXECUTE FUNCTION count_subscription_insert();
        "#,
    )
    .execute(&server.backend.store.primary)
    .await?;
    let adds = topics(NOTIFICATION_BATCH_TOPICS + 1)
        .map(|topic| api::Subscription {
            topic,
            hmac_epoch_base: 0,
            hmac_keys: vec![],
            include_commits: false,
        })
        .collect();
    let error = client
        .update_subscriptions(update(adds, vec![]))
        .await
        .unwrap_err();
    status(error, Code::InvalidArgument, BATCH_LIMIT);
    let attempted: bool =
        sqlx::query_scalar("SELECT is_called FROM attempted_subscription_inserts")
            .fetch_one(&server.backend.store.primary)
            .await?;
    assert!(!attempted);
    assert_count(&server, 0).await;
    server.stop().await?;
}

/// The bound precedes the recipient lookup, so an unknown recipient gets
/// INVALID_ARGUMENT and not NOT_FOUND.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PUSH-264
async fn subscription_batch_bound_precedes_recipient_lookup() {
    let server = TestServer::new(configured).await?;
    let removes = topics(NOTIFICATION_BATCH_TOPICS + 1).collect();
    let error = server
        .notifications()
        .update_subscriptions(update(vec![], removes))
        .await
        .unwrap_err();
    status(error, Code::InvalidArgument, BATCH_LIMIT);
    server.stop().await?;
}

/// Entries past the bound are never decoded: a request that would fail to
/// decode gets the bound's status, not the decoder's.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PUSH-264
async fn subscription_batch_bound_precedes_decode() {
    let server = TestServer::new(configured).await?;
    let mut message =
        update(vec![], topics(NOTIFICATION_BATCH_TOPICS + 1).collect()).encode_to_vec();
    message.push(0xff);
    let error = raw_unary(&server, UPDATE, &[], framed(&message), true).await?;
    status(error, Code::InvalidArgument, BATCH_LIMIT);
    server.stop().await?;
}

/// Keys count across adds, before decode.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PUSH-264
async fn subscription_keys_are_rejected_before_decode() {
    let server = TestServer::new(configured).await?;
    let mut add = subscription(1, 0);
    add.hmac_keys = vec![vec![1; HMAC_KEY_BYTES]; NOTIFICATION_BATCH_HMAC_KEYS / 2 + 1];
    let mut other = add.clone();
    other.topic = subscription(2, 0).topic.clone();
    let mut message = update(vec![add, other], vec![]).encode_to_vec();
    message.push(0xff);
    let error = raw_unary(&server, UPDATE, &[], framed(&message), true).await?;
    status(
        error,
        Code::InvalidArgument,
        "request exceeds the subscription key limit",
    );
    server.stop().await?;
}

/// Tonic decodes every message of a unary body and hands the handler the
/// first, so without the gate the first update would apply.
#[xmtp_common::test(unwrap_try = true)]
async fn subscription_second_message_is_rejected() {
    let server = TestServer::new(configured).await?;
    server.notifications().register(registration()).await?;
    let message = framed(&update(vec![subscription(1, 3)], vec![]).encode_to_vec());
    let error = raw_unary(
        &server,
        UPDATE,
        &[],
        [message.clone(), message].concat(),
        true,
    )
    .await?;
    status(
        error,
        Code::InvalidArgument,
        "request carries more than one message",
    );
    assert_count(&server, 0).await;
    server.stop().await?;
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
        .update_subscriptions(update(vec![], vec![subscription(1, 0).topic.clone()]))
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
