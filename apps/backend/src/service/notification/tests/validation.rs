use super::*;

fn webhook(url: &str, signing_key: Vec<u8>) -> api::RegisterRequest {
    {
        let mut request = registration();
        request.delivery = Some(api::register_request::Delivery::Http(api::HttpDelivery {
            url: url.into(),
            signing_key,
        }));
        request
    }
}

// verifies: PUSH-253
#[xmtp_common::test(unwrap_try = true)]
async fn ownership_checks_precede_payload_checks_on_each_method() {
    let server = TestServer::new(configured).await?;
    let mut client = server.notifications();
    for (id, secret) in [
        (vec![], vec![0; 32]),
        (vec![0; 32], vec![]),
        (vec![0; 33], vec![0; 32]),
        (vec![0; 32], vec![0; 33]),
    ] {
        status(
            client
                .register({
                    let mut request = registration();
                    request.recipient_id = id.clone();
                    request.recipient_secret = secret.clone();
                    request
                })
                .await
                .unwrap_err(),
            Code::InvalidArgument,
            MALFORMED_REQUEST,
        );
        status(
            client
                .unregister(api::UnregisterRequest {
                    recipient_id: id.clone(),
                    recipient_secret: secret.clone(),
                })
                .await
                .unwrap_err(),
            Code::InvalidArgument,
            MALFORMED_REQUEST,
        );
        status(
            client
                .update_subscriptions(api::UpdateSubscriptionsRequest {
                    recipient_id: id,
                    recipient_secret: secret,
                    adds: vec![api::Subscription::default()],
                    removes: vec![],
                })
                .await
                .unwrap_err(),
            Code::InvalidArgument,
            MALFORMED_REQUEST,
        );
    }
    status(
        client.unregister(unregister()).await.unwrap_err(),
        Code::NotFound,
        "recipient is not registered",
    );
    status(
        client
            .update_subscriptions(update(vec![api::Subscription::default()], vec![]))
            .await
            .unwrap_err(),
        Code::NotFound,
        "recipient is not registered",
    );
    client.register(registration()).await?;
    let mut wrong = registration();
    wrong.recipient_secret = vec![99; 32];
    wrong.delivery = Some(api::register_request::Delivery::Apns(api::ApnsDelivery {
        token: "unused".into(),
    }));
    status(
        client.register(wrong).await.unwrap_err(),
        Code::PermissionDenied,
        "recipient secret is not valid",
    );
    let mut wrong = unregister();
    wrong.recipient_secret = vec![99; 32];
    status(
        client.unregister(wrong).await.unwrap_err(),
        Code::PermissionDenied,
        "recipient secret is not valid",
    );
    let mut wrong = update(vec![api::Subscription::default()], vec![]);
    wrong.recipient_secret = vec![99; 32];
    status(
        client.update_subscriptions(wrong).await.unwrap_err(),
        Code::PermissionDenied,
        "recipient secret is not valid",
    );
    assert_count(&server, 0).await;
    server.stop().await?;
}

// verifies: PUSH-210, PUSH-256
#[xmtp_common::test(unwrap_try = true)]
async fn delivery_checks_follow_channel_key_shape_and_url_order() {
    let server = TestServer::new(|_| {}).await?;
    let mut client = server.notifications();
    for delivery in [
        api::register_request::Delivery::Apns(api::ApnsDelivery {
            token: "token".into(),
        }),
        api::register_request::Delivery::Fcm(api::FcmDelivery {
            token: "token".into(),
        }),
        api::register_request::Delivery::Http(api::HttpDelivery {
            url: "http://localhost".into(),
            signing_key: vec![],
        }),
    ] {
        status(
            client
                .register({
                    let mut request = registration();
                    request.delivery = Some(delivery);
                    request
                })
                .await
                .unwrap_err(),
            Code::FailedPrecondition,
            "channel is not configured",
        );
    }
    server.stop().await?;
    let server = TestServer::new(configured).await?;
    let mut client = server.notifications();
    for url in [
        "invalid",
        "http://127.0.0.1",
        "https:///",
        "https://user:secret@127.0.0.1",
    ] {
        status(
            client
                .register(webhook(url, vec![1; 32]))
                .await
                .unwrap_err(),
            Code::InvalidArgument,
            "webhook url is not allowed",
        );
    }
    // A request that fails a check needing no lookup is never UNAVAILABLE.
    let lookup_fails = "https://lookup-fails.invalid/";
    for (request, message) in [
        (
            webhook(lookup_fails, vec![1; 15]),
            "webhook signing key length is not allowed",
        ),
        (
            webhook(
                &format!(
                    "{lookup_fails}{}",
                    "a".repeat(MAX_DELIVERY_CHARACTERS + 1 - lookup_fails.len())
                ),
                vec![1; 32],
            ),
            MALFORMED_REQUEST,
        ),
    ] {
        status(
            client.register(request).await.unwrap_err(),
            Code::InvalidArgument,
            message,
        );
    }
    for length in [0, 15, 65] {
        let request = webhook("https://127.0.0.1/hook", vec![1; length]);
        status(
            client.register(request).await.unwrap_err(),
            Code::InvalidArgument,
            "webhook signing key length is not allowed",
        );
    }
    for length in [16, 64] {
        let request = webhook("https://127.0.0.1/hook", vec![1; length]);
        assert_eq!(
            client.register(request).await?.into_inner().channel,
            api::Channel::Http as i32
        );
    }
    for token in [
        String::new(),
        "a".repeat(MAX_DELIVERY_CHARACTERS + 1),
        "a\0b".into(),
    ] {
        let request = {
            let mut request = registration();
            request.delivery = Some(api::register_request::Delivery::Fcm(api::FcmDelivery {
                token,
            }));
            request
        };
        status(
            client.register(request).await.unwrap_err(),
            Code::InvalidArgument,
            MALFORMED_REQUEST,
        );
    }
    status(
        client
            .register({
                let mut request = registration();
                request.delivery = None;
                request
            })
            .await
            .unwrap_err(),
        Code::InvalidArgument,
        MALFORMED_REQUEST,
    );
    server.stop().await?;
}

// verifies: PUSH-215
#[xmtp_common::test(unwrap_try = true)]
async fn every_subscription_shape_failure_leaves_the_recipient_unchanged() {
    let server = TestServer::new(|config| {
        configured(config);
        config.limits.max_push_topics = 1;
    })
    .await?;
    let mut client = server.notifications();
    client.register(registration()).await?;
    client
        .update_subscriptions(update(vec![subscription(1, 1)], vec![]))
        .await?;
    let original = server
        .backend
        .store
        .load_recipient(&registration().recipient_id.clone())
        .await?
        .unwrap()
        .renewed_ns;
    let mut invalid = Vec::new();
    for topic in [
        vec![],
        vec![0],
        vec![0; 16],
        vec![0; 17],
        vec![0; 18],
        vec![1; 32],
        vec![1; 34],
        vec![2; 33],
        vec![255; 33],
    ] {
        invalid.push(update(
            vec![{
                let mut request = subscription(2, 0);
                request.topic = topic;
                request
            }],
            vec![],
        ));
    }
    invalid.push(update(vec![subscription(2, 4)], vec![]));
    for length in [0, 41, 43] {
        invalid.push(update(
            vec![{
                let mut request = subscription(2, 0);
                request.hmac_keys = vec![vec![0; length]];
                request
            }],
            vec![],
        ));
    }
    invalid.push(update(
        vec![{
            let mut request = subscription(2, 0);
            request.hmac_epoch_base = -1;
            request
        }],
        vec![],
    ));
    invalid.push(update(vec![subscription(2, 0), subscription(2, 0)], vec![]));
    invalid.push(update(
        vec![],
        vec![
            subscription(1, 0).topic.clone(),
            subscription(1, 0).topic.clone(),
        ],
    ));
    invalid.push(update(
        vec![subscription(1, 0)],
        vec![subscription(1, 0).topic.clone()],
    ));
    invalid.push(update(vec![], vec![vec![2; 33]]));
    // A legacy 16-byte group id names no topic.
    invalid.push(update(
        vec![],
        vec![TopicKind::GroupMessagesV1.create([1; 16]).to_vec()],
    ));
    for request in invalid {
        status(
            client.update_subscriptions(request).await.unwrap_err(),
            Code::InvalidArgument,
            MALFORMED_SUBSCRIPTION,
        );
        assert_count(&server, 1).await;
        assert_eq!(
            server
                .backend
                .store
                .load_recipient(&registration().recipient_id.clone())
                .await?
                .unwrap()
                .renewed_ns,
            original
        );
    }
    client
        .update_subscriptions(update(
            vec![{
                let mut request = api::Subscription::default();
                request.topic = TopicKind::WelcomeMessagesV1.create([7; 32]).to_vec();
                request
            }],
            vec![subscription(1, 0).topic.clone()],
        ))
        .await?;
    assert_count(&server, 1).await;
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn webhook_absent_or_empty_domain_list_allows_public_hosts_and_blocks_unsafe_urls() {
    for allowed_domains in [None, Some(vec![])] {
        let server = TestServer::new(|config| {
            config.push.http = Some(HttpConfig {
                allowed_domains,
                ..HttpConfig::default()
            })
        })
        .await?;
        let mut client = server.notifications();
        for url in [
            "https://localhost/hook",
            "https://10.0.0.1",
            "https://[::1]",
            "https://[fec0::1]",
            "https://[::ffff:127.0.0.1]",
            "http://8.8.8.8/hook",
            "https://user:password@8.8.8.8/hook",
        ] {
            status(
                client
                    .register(webhook(url, vec![1; 32]))
                    .await
                    .unwrap_err(),
                Code::InvalidArgument,
                "webhook url is not allowed",
            );
        }
        status(
            client
                .register(webhook("https://lookup-fails.invalid/hook", vec![1; 32]))
                .await
                .unwrap_err(),
            Code::Unavailable,
            "webhook host lookup failed",
        );
        client
            .register(webhook("https://8.8.8.8/hook", vec![1; 32]))
            .await?;
        server.stop().await?;
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn webhook_populated_domain_list_restricts_hosts() {
    let server = TestServer::new(|config| {
        config.push.http = Some(HttpConfig {
            allowed_domains: Some(vec!["LOCALHOST".into()]),
            allow_private_addresses: true,
        })
    })
    .await?;
    server
        .notifications()
        .register(webhook("https://localhost/hook", vec![1; 32]))
        .await?;
    status(
        server
            .notifications()
            .register(webhook("https://127.0.0.1/hook", vec![1; 32]))
            .await
            .unwrap_err(),
        Code::InvalidArgument,
        "webhook url is not allowed",
    );
    server.stop().await?;
}

// verifies: PUSH-256
#[xmtp_common::test(unwrap_try = true)]
fn webhook_blocked_address_classes() {
    for (address, expected) in [
        ("8.8.8.8", false),
        ("127.0.0.1", true),
        ("172.16.0.1", true),
        ("192.168.0.1", true),
        ("169.254.169.254", true),
        ("0.0.0.0", true),
        ("::", true),
        ("fc00::1", true),
        ("fe80::1", true),
        ("100.63.255.255", false),
        ("100.64.0.0", true),
        ("100.100.100.100", true),
        ("100.127.255.255", true),
        ("100.128.0.0", false),
        ("::ffff:100.64.0.0", true),
        ("::ffff:100.127.255.255", true),
        ("::ffff:8.8.8.8", true),
        ("fec0::1", true),
        ("feff:ffff:ffff:ffff:ffff:ffff:ffff:ffff", true),
        ("64:ff9b::a00:1", true),
        ("64:ff9b:1::a00:1", true),
        ("64:ff9b::808:808", false),
    ] {
        assert_eq!(
            webhook_url::blocked(address.parse()?, &[]),
            expected,
            "{address}"
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn wildcard_hosts_require_a_complete_label_and_ignore_case() {
    for (host, domain, expected) in [
        ("hooks.example.com", "hooks.example.com", true),
        ("HOOKS.example.com", "hooks.EXAMPLE.com", true),
        ("a.example.org", "*.example.org", true),
        ("a.b.example.org", "*.EXAMPLE.org", true),
        ("example.org", "*.example.org", false),
        ("evilexample.org", "*.example.org", false),
        ("a.example.org.evil", "*.example.org", false),
    ] {
        assert_eq!(webhook_url::matches_domain(host, domain), expected);
    }
}
