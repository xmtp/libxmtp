use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn initial_connection_can_reconnect_before_connected() {
    use crate::ConnectionState;

    let mut previous_states = Vec::new();
    let mut states = [ConnectionState::Reconnecting, ConnectionState::Connected].into_iter();
    let connected = wait_for_initial_connection(|previous| {
        previous_states.push(previous);
        std::future::ready(Ok(states.next().expect("next initial state")))
    })
    .await?;
    assert_eq!(connected, ConnectionState::Connected);
    assert_eq!(
        previous_states,
        [ConnectionState::Connecting, ConnectionState::Reconnecting]
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_connection_state_waits_release_reader_workers() {
    use crate::ConnectionState;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let messages = group.message_reader(None).await?;
    let conversations = client.conversations().conversation_reader(None).await?;
    xmtp_common::time::timeout(Duration::from_secs(20), async {
        tokio::try_join!(
            wait_for_initial_connection(|previous| messages.connection_state_changed(previous)),
            wait_for_initial_connection(|previous| conversations.connection_state_changed(previous))
        )
    })
    .await??;

    let control = messages.control_for_test();
    let message_count = control.lease_holder_count_for_test();
    let conversation_count = Arc::strong_count(conversations.lease_for_test());
    for _ in 0..3 {
        assert!(
            xmtp_common::time::timeout(
                Duration::from_millis(100),
                messages.connection_state_changed(ConnectionState::Connected)
            )
            .await
            .is_err()
        );
        assert!(
            xmtp_common::time::timeout(
                Duration::from_millis(100),
                conversations.connection_state_changed(ConnectionState::Connected)
            )
            .await
            .is_err()
        );
    }
    xmtp_common::time::timeout(Duration::from_secs(1), async {
        while control.lease_holder_count_for_test() != message_count
            || Arc::strong_count(conversations.lease_for_test()) != conversation_count
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    messages.end().await?;
    conversations.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn connection_state_across_toxiproxy_drop() {
    use crate::ConnectionState;
    use futures::FutureExt;
    use std::panic::AssertUnwindSafe;

    // Set keepalive before the first transport starts in this test process.
    unsafe {
        std::env::set_var("XMTP_GRPC_KEEPALIVE_INTERVAL_SECS", "5");
        std::env::set_var("XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS", "5");
    }
    xmtp_common::toxiproxy_test(async || {
        let proxy = xmtp_common::toxiproxy()
            .find_proxy("backend")
            .await
            .expect("backend proxy");
        let mut config = options();
        if let Some(BackendSource::Options { options }) = &mut config.backend {
            options.url = xmtp_configuration::backend_test_toxic_url();
        }
        let client = Client::create(crate::generate_local_signer().await, config)
            .await
            .expect("client");
        let group = client
            .conversations()
            .create_group(vec![], None)
            .await
            .expect("group");
        let messages = group.message_reader(None).await.expect("message reader");
        let conversations = client
            .conversations()
            .conversation_reader(None)
            .await
            .expect("conversation reader");

        for state in [
            messages.connection_state().await,
            conversations.connection_state().await,
        ] {
            assert!(matches!(
                state,
                ConnectionState::Connecting | ConnectionState::Connected
            ));
        }
        let connected = async {
            let (message, conversation) = tokio::join!(
                wait_for_initial_connection(|previous| messages.connection_state_changed(previous)),
                wait_for_initial_connection(
                    |previous| conversations.connection_state_changed(previous)
                )
            );
            (
                message.expect("message connected"),
                conversation.expect("conversation connected"),
            )
        };
        let (message, conversation) =
            xmtp_common::time::timeout(Duration::from_secs(20), connected)
                .await
                .expect("initial connection");
        assert_eq!(message, ConnectionState::Connected);
        assert_eq!(conversation, ConnectionState::Connected);

        // Keep the stream's shared observer busy while the state watcher waits.
        let shared_wait = conversations
            .lease_for_test()
            .lock_change_receiver_for_test()
            .await;
        // Drain a stored permit, then wait until next() reaches its idle wait.
        let idle = conversations.idle_read_for_test();
        let _ = idle.notified().now_or_never();
        let pending_conversation = conversations.clone();
        let pending_read = tokio::spawn(async move { pending_conversation.next().await });
        xmtp_common::time::timeout(Duration::from_secs(5), idle.notified())
            .await
            .expect("conversation read reached its idle wait");
        assert!(
            !pending_read.is_finished(),
            "conversation read was not pending"
        );
        let state_messages = messages.clone();
        let state_conversations = conversations.clone();
        let pending_states = tokio::spawn(async move {
            tokio::join!(
                state_messages.connection_state_changed(ConnectionState::Connected),
                state_conversations.connection_state_changed(ConnectionState::Connected)
            )
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !pending_states.is_finished(),
            "state observer was not pending"
        );

        proxy.disable().await.expect("disable proxy");
        xmtp_common::time::timeout(Duration::from_secs(30), async {
            while conversations.connection_state().await != ConnectionState::Reconnecting {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("conversation reader saw reconnecting");
        let outage = AssertUnwindSafe(async {
            let (message, conversation) =
                xmtp_common::time::timeout(Duration::from_secs(3), pending_states)
                    .await
                    .expect("connection drop")
                    .expect("state observer task");
            assert_eq!(
                message.expect("message drop"),
                ConnectionState::Reconnecting
            );
            assert_eq!(
                conversation.expect("conversation drop"),
                ConnectionState::Reconnecting
            );
        })
        .catch_unwind()
        .await;
        drop(shared_wait);
        proxy.enable().await.expect("restore proxy");
        outage.expect("connection drop assertion");

        let (message, conversation) = xmtp_common::time::timeout(Duration::from_secs(30), async {
            tokio::join!(
                messages.connection_state_changed(ConnectionState::Reconnecting),
                conversations.connection_state_changed(ConnectionState::Reconnecting)
            )
        })
        .await
        .expect("connection recovery");
        assert_eq!(
            message.expect("message recovery"),
            ConnectionState::Connected
        );
        assert_eq!(
            conversation.expect("conversation recovery"),
            ConnectionState::Connected
        );
        messages.end().await.expect("end message reader");
        conversations.end().await.expect("end conversation reader");
        assert!(
            xmtp_common::time::timeout(Duration::from_secs(5), pending_read)
                .await
                .expect("pending conversation read settles")
                .expect("pending conversation task")
                .expect("pending conversation result")
                .is_none()
        );
        client.end().await.expect("end client");
    })
    .await;
}
