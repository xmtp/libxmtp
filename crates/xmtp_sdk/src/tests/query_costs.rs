use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn conversation_list_state_and_last_activity() {
    use crate::{Conversation, ConversationOrder, ListConversationsOptions};
    use xmtp_db::{count_sql_queries, sql_key_store::count_kv_reads};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let older = client.conversations().create_group(vec![], None).await?;
    let newer = client.conversations().create_group(vec![], None).await?;
    let sent = older.send_text("most recent".into(), None).await?;
    let stored_sent_at_ns = client.inner.message(sent.to_bytes()?)?.sent_at_ns;
    let ordered = client
        .conversations()
        .list(Some(ListConversationsOptions {
            order_by: Some(ConversationOrder::LastActivity),
            ..Default::default()
        }))
        .await?;
    let ids = ordered
        .into_iter()
        .map(|conversation| match conversation {
            Conversation::Group { group } => group.id(),
            Conversation::Dm { dm } => dm.id(),
        })
        .collect::<Vec<_>>();
    let core_ids = client
        .inner
        .list_conversations(xmtp_db::group::GroupQueryArgs {
            order_by: Some(xmtp_db::group::GroupQueryOrderBy::LastActivity),
            ..Default::default()
        })?
        .into_iter()
        .map(|item| ConversationId::from(item.group.group_id))
        .collect::<Vec<_>>();
    assert_eq!(ids, core_ids);
    let older_activity = older.last_activity_at(None).await?.0;
    assert_eq!(older_activity, stored_sent_at_ns);
    let newer_activity = newer.last_activity_at(None).await?.0;
    let first_activity = if ids.first() == Some(&older.id()) {
        older_activity
    } else {
        assert_eq!(ids.first(), Some(&newer.id()));
        newer_activity
    };
    let second_activity = if ids.get(1) == Some(&older.id()) {
        older_activity
    } else {
        assert_eq!(ids.get(1), Some(&newer.id()));
        newer_activity
    };
    assert!(first_activity >= second_activity);
    assert_eq!(
        older.last_activity_at(Some(vec![])).await?,
        older.created_at()
    );
    let text_type = crate::encode_text("filter".into())?.r#type;
    assert_eq!(
        older.last_activity_at(Some(vec![text_type])).await?,
        older.last_activity_at(None).await?
    );

    let ((snapshot, core_kv_reads), core_queries, core_writes) =
        count_sql_queries(|| count_kv_reads(|| older.inner.state_snapshot()));
    let snapshot = snapshot?;
    let facade_state = older.state().await?;
    assert_eq!(
        facade_state.name,
        snapshot.group.expect("group metadata").name
    );
    let (queries, kv_reads, writes) = *older.state_counts.lock();
    assert!(
        queries <= 3,
        "facade state read used {queries} SQL queries and {kv_reads} key reads"
    );
    assert!(kv_reads <= 2, "state read used {kv_reads} key-value reads");
    assert_eq!(writes, 0, "state read began a write transaction");
    assert!(core_queries.saturating_sub(core_kv_reads) <= 3);
    assert!(core_kv_reads <= 2);
    assert_eq!(core_writes, 0);
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn conversation_list_lift_uses_bounded_queries() {
    use xmtp_db::{count_sql_queries, sql_key_store::count_kv_reads};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    for _ in 0..3 {
        client.conversations().create_group(vec![], None).await?;
    }
    let ((result, kv_reads), queries, writes) = count_sql_queries(|| {
        count_kv_reads(|| {
            futures::executor::block_on(crate::conversation::list_local(
                client.inner.clone(),
                client.key,
                Default::default(),
            ))
        })
    });
    assert!(result?.len() >= 3);
    assert!(queries <= 3, "list used {queries} SQL queries");
    assert!(kv_reads <= 1, "list used {kv_reads} key-value reads");
    assert_eq!(writes, 0);
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn message_history_queries_do_not_grow_per_row() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let first = group.send_text("first".into(), None).await?;
    assert_eq!(group.messages(None).await?.len(), 1);
    let one_query_count = *group.history_query_count.lock();

    for number in 0..3 {
        group.send_text(format!("more {number}"), None).await?;
    }
    client
        .conversations()
        .reply_to_message(first, crate::encode_text("reply".into())?, None)
        .await?;
    assert_eq!(group.messages(None).await?.len(), 5);
    let many_query_count = *group.history_query_count.lock();
    assert!(
        many_query_count <= one_query_count + 2,
        "history used {one_query_count} queries for one row and {many_query_count} for five rows"
    );
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn conversation_list_created_filters_select_exact_ids() {
    use crate::{Conversation, ListConversationsOptions, Timestamp};
    use std::collections::HashSet;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let first = client.conversations().create_group(vec![], None).await?;
    let second = client.conversations().create_group(vec![], None).await?;
    let third = client.conversations().create_group(vec![], None).await?;
    assert!(first.created_at().0 < second.created_at().0);
    assert!(second.created_at().0 < third.created_at().0);

    let list = |created_after: Option<Timestamp>, created_before: Option<Timestamp>| {
        let conversations = client.conversations();
        async move {
            let listed = conversations
                .list(Some(ListConversationsOptions {
                    created_after,
                    created_before,
                    ..Default::default()
                }))
                .await?;
            Ok::<_, crate::XmtpError>(
                listed
                    .into_iter()
                    .map(|conversation| match conversation {
                        Conversation::Group { group } => group.id(),
                        Conversation::Dm { dm } => dm.id(),
                    })
                    .collect::<HashSet<_>>(),
            )
        }
    };
    let ids = |groups: &[&crate::Group]| groups.iter().map(|g| g.id()).collect::<HashSet<_>>();

    // Both bounds are exclusive (`g.created_at_ns > ?` and `< ?`).
    assert_eq!(
        list(Some(first.created_at()), None).await?,
        ids(&[&second, &third])
    );
    assert_eq!(
        list(None, Some(third.created_at())).await?,
        ids(&[&first, &second])
    );
    assert_eq!(
        list(Some(first.created_at()), Some(third.created_at())).await?,
        ids(&[&second])
    );
    client.end().await?;
}
