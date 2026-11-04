//! The leaves of `subscriptions/recovery.rs` that need the client.

use crate::groups::test_support::terminal_source;

/// A blocked connection reached through each production route is never
/// retried and needs no reconnect. The table records, per route, whether
/// stream recovery treats the returned error as a terminal source. The
/// stream and local-delivery wrappers forward `source()` past the
/// `BlockedConnection`, so they are not terminal sources and end recovery
/// only because they are not retryable.
// verifies: CONF-075
#[xmtp_common::test(unwrap_try = true)]
async fn blocked_connection_is_terminal_and_never_retryable() {
    use crate::{
        context::CoreContext,
        server_configuration::BlockedConnection,
        subscriptions::{
            local_delivery::{DeliveryScope, LocalDeliveryFilter},
            message_reader::MessageReader,
            stream_conversations::StreamConversations,
        },
        tester,
        worker::NeedsDbReconnect,
    };
    use xmtp_common::RetryableError;

    for reason in [
        BlockedConnection::BackendMismatch {
            stored: "first".into(),
            received: "second".into(),
        },
        BlockedConnection::ClientVersionTooOld {
            client: "1.0.0".into(),
            minimum: "9999.0.0".into(),
        },
    ] {
        tester!(alix, disable_workers);
        let group = alix.create_group(None, None)?;
        alix.context
            .server_configuration()
            .block_connection(reason.clone());
        // (route, retryable, needs reconnect, terminal source)
        let mut seen: Vec<(&str, bool, Option<bool>, bool)> = vec![];

        let error = group.sync().await.unwrap_err();
        seen.push((
            "group sync",
            error.is_retryable(),
            Some(error.needs_db_reconnect()),
            terminal_source(&error),
        ));
        let error = alix.catch_up_to_live(None).await.unwrap_err();
        seen.push((
            "catch-up",
            error.is_retryable(),
            None,
            terminal_source(&error),
        ));
        let error = StreamConversations::new(&alix.context, None, false, None)
            .await
            .err()
            .expect("a blocked client cannot open a conversation stream");
        seen.push((
            "conversation stream",
            error.is_retryable(),
            Some(error.needs_db_reconnect()),
            terminal_source(&error),
        ));
        let error = MessageReader::new(
            alix.context.clone(),
            DeliveryScope::All,
            LocalDeliveryFilter::default(),
            None,
        )
        .err()
        .expect("a blocked client cannot open local delivery");
        seen.push((
            "local delivery",
            error.is_retryable(),
            None,
            terminal_source(&error),
        ));
        let error = alix
            .context
            .api()
            .query_group_messages(group.group_id)
            .await
            .unwrap_err();
        seen.push((
            "request preflight",
            error.is_retryable(),
            None,
            terminal_source(&error),
        ));

        assert_eq!(
            seen,
            vec![
                ("group sync", false, Some(false), true),
                ("catch-up", false, None, true),
                ("conversation stream", false, Some(false), false),
                ("local delivery", false, None, false),
                ("request preflight", false, None, false),
            ],
            "{reason:?}"
        );
    }
}
