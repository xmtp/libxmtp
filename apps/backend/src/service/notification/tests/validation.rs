use super::*;

fn webhook(url: &str, signing_key: Vec<u8>) -> api::RegisterRequest {
    api::RegisterRequest {
        delivery: Some(api::register_request::Delivery::Http(api::HttpDelivery {
            url: url.into(),
            signing_key,
        })),
        ..registration()
    }
}

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
                .register(api::RegisterRequest {
                    recipient_id: id.clone(),
                    recipient_secret: secret.clone(),
                    ..registration()
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

#[xmtp_common::test(unwrap_try = true)]
async fn delivery_checks_follow_channel_url_and_key_order() {
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
                .register(api::RegisterRequest {
                    delivery: Some(delivery),
                    ..registration()
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
            client.register(webhook(url, vec![])).await.unwrap_err(),
            Code::InvalidArgument,
            "webhook url is not allowed",
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
        let request = api::RegisterRequest {
            delivery: Some(api::register_request::Delivery::Fcm(api::FcmDelivery {
                token,
            })),
            ..registration()
        };
        status(
            client.register(request).await.unwrap_err(),
            Code::InvalidArgument,
            MALFORMED_REQUEST,
        );
    }
    status(
        client
            .register(api::RegisterRequest {
                delivery: None,
                ..registration()
            })
            .await
            .unwrap_err(),
        Code::InvalidArgument,
        MALFORMED_REQUEST,
    );
    server.stop().await?;
}

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
        .load_recipient(&registration().recipient_id)
        .await?
        .unwrap()
        .renewed_ns;
    let mut invalid = Vec::new();
    for topic in [
        vec![],
        vec![0],
        vec![0; 16],
        vec![0; 18],
        vec![1; 32],
        vec![1; 34],
        vec![2; 33],
        vec![255; 33],
    ] {
        invalid.push(update(
            vec![api::Subscription {
                topic,
                ..subscription(2, 0)
            }],
            vec![],
        ));
    }
    invalid.push(update(vec![subscription(2, 4)], vec![]));
    for length in [0, 41, 43] {
        invalid.push(update(
            vec![api::Subscription {
                hmac_keys: vec![vec![0; length]],
                ..subscription(2, 0)
            }],
            vec![],
        ));
    }
    invalid.push(update(
        vec![api::Subscription {
            hmac_epoch_base: -1,
            ..subscription(2, 0)
        }],
        vec![],
    ));
    invalid.push(update(vec![subscription(2, 0), subscription(2, 0)], vec![]));
    invalid.push(update(
        vec![],
        vec![subscription(1, 0).topic, subscription(1, 0).topic],
    ));
    invalid.push(update(
        vec![subscription(1, 0)],
        vec![subscription(1, 0).topic],
    ));
    invalid.push(update(vec![], vec![vec![2; 33]]));
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
                .load_recipient(&registration().recipient_id)
                .await?
                .unwrap()
                .renewed_ns,
            original
        );
    }
    client
        .update_subscriptions(update(
            vec![api::Subscription {
                topic: TopicKind::WelcomeMessagesV1.create([7; 32]).to_vec(),
                ..Default::default()
            }],
            vec![subscription(1, 0).topic],
        ))
        .await?;
    assert_count(&server, 1).await;
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn webhook_absent_domain_list_allows_public_hosts_and_blocks_private_addresses() {
    let server = TestServer::new(|config| config.push.http = Some(HttpConfig::default())).await?;
    let mut client = server.notifications();
    for url in [
        "https://localhost/hook",
        "https://127.0.0.1",
        "https://10.0.0.1",
        "https://100.64.0.0",
        "https://100.100.100.100",
        "https://100.127.255.255",
        "https://[::ffff:100.100.100.100]",
        "https://172.16.0.1",
        "https://192.168.0.1",
        "https://169.254.169.254",
        "https://0.0.0.0",
        "https://[::1]",
        "https://[::]",
        "https://[fc00::1]",
        "https://[fe80::1]",
        "https://[fec0::1]",
        "https://[::ffff:127.0.0.1]",
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
    client
        .register(webhook("https://8.8.8.8/hook", vec![1; 32]))
        .await?;
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn webhook_empty_domain_list_allows_public_hosts_and_preserves_url_safety_checks() {
    let server = TestServer::new(|config| {
        config.push.http = Some(HttpConfig {
            allowed_domains: Some(vec![]),
            ..HttpConfig::default()
        })
    })
    .await?;
    let mut client = server.notifications();
    client
        .register(webhook("https://8.8.8.8/hook", vec![1; 32]))
        .await?;
    for url in [
        "https://127.0.0.1/hook",
        "https://10.0.0.1/hook",
        "https://[::1]/hook",
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
    server.stop().await?;
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

#[xmtp_common::test(unwrap_try = true)]
fn webhook_shared_address_filter_blocks_the_exact_range_and_mapped_ipv4() {
    for (address, expected) in [
        ("100.63.255.255", false),
        ("100.64.0.0", true),
        ("100.100.100.100", true),
        ("100.127.255.255", true),
        ("100.128.0.0", false),
        ("::ffff:100.63.255.255", false),
        ("::ffff:100.64.0.0", true),
        ("::ffff:100.127.255.255", true),
        ("::ffff:100.128.0.0", false),
    ] {
        assert_eq!(
            webhook_url::blocked(address.parse()?),
            expected,
            "{address}"
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn webhook_deprecated_site_local_filter_matches_exact_range() {
    for (address, expected) in [
        ("febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff", false),
        ("fec0::", true),
        ("feff:ffff:ffff:ffff:ffff:ffff:ffff:ffff", true),
        ("ff00::", false),
    ] {
        assert_eq!(
            webhook_url::deprecated_site_local(address.parse()?),
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
