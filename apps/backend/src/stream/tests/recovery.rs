use crate::{
    api,
    test_support::{
        self, RunningServer, TestServer,
        native::{Native, envelope},
    },
};

#[xmtp_common::test(unwrap_try = true)]
async fn reconnect_to_independent_instance_replays_received_but_unprocessed_rows() {
    let mut first = TestServer::new(|config| config.streams.poll_interval_ms = 10).await?;
    let metas = first
        .publish((1..=4).map(|value| envelope(96, value)).collect())
        .await?;
    let topic = metas[0].topic.clone().unwrap();
    let mut original = Native::open(&first).await?;
    original
        .update(1, vec![test_support::query_topic(topic.clone(), 0)], vec![])
        .await?;
    assert!(matches!(
        original.next().await?,
        api::subscribe_response::Response::Applied(_)
    ));
    assert_eq!(original.messages(4).await?.len(), 4);
    drop(original);
    first.shutdown();
    first.wait_stopped().await?;
    let mut second = RunningServer::new((*first.backend.config).clone()).await?;
    let mut native = Native::open(&second).await?;
    native
        .update(1, vec![test_support::query_topic(topic.clone(), 2)], vec![])
        .await?;
    assert!(
        matches!(native.next().await?, api::subscribe_response::Response::Applied(applied) if applied.added_targets[0].through_sequence_id == 4)
    );
    let rows = native.messages(2).await?;
    assert_eq!(
        rows.iter()
            .map(|row| row
                .meta
                .as_ref()
                .unwrap()
                .cursor
                .as_ref()
                .unwrap()
                .sequence_id)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    let mut client =
        api::subscription_service_client::SubscriptionServiceClient::new(second.channel.clone());
    let mut static_stream = client
        .subscribe_static(api::SubscribeStaticRequest {
            topics: vec![test_support::query_topic(topic, 2)],
        })
        .await?
        .into_inner();
    assert!(matches!(
        static_stream.message().await?.unwrap().response,
        Some(api::subscribe_static_response::Response::Started(_))
    ));
    let Some(api::subscribe_static_response::Response::Messages(messages)) =
        static_stream.message().await?.unwrap().response
    else {
        panic!("expected static replay");
    };
    assert_eq!(messages.envelopes, rows);
    drop(native);
    drop(static_stream);
    second.stop().await?;
    first.stop().await?;
}
