use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn get_message_by_id_errors_on_unconvertible_row() {
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let id = group.send_text("valid".into(), None).await?;
    let id_bytes = id.to_bytes()?;
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(&id_bytes)))
            .set(dsl::sender_inbox_id.eq(""))
            .execute(conn)
    })?;
    let result = client.conversations().get_message_by_id(id).await;
    assert!(
        result.is_err(),
        "a stored row with an unconvertible sender_inbox_id must surface an error, not None: {result:?}"
    );
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_skips_bad_row_and_warns_without_content() {
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};
    use xmtp_logging::{Level, test_logging::LogCapture};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let good = group.send_text("good row".into(), None).await?;
    let bad = group
        .send_text("sensitive-history-content".into(), None)
        .await?;
    let bad_bytes = bad.to_bytes()?;
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(&bad_bytes)))
            .set(dsl::sender_inbox_id.eq(""))
            .execute(conn)
    })?;

    let messages = group.messages(None).await?;
    assert!(messages.iter().any(|message| message.0.id == good));
    assert!(!messages.iter().any(|message| message.0.id == bad));

    let enriched = group
        .inner
        .find_messages_v2_with_stored(&MsgQueryArgs::default())?;
    let capture = LogCapture::new(Level::Warn);
    let lifted = tracing::dispatcher::with_default(&capture.dispatch(), || {
        crate::conversation::lift_history_messages(enriched, client.client_key())
    });
    assert!(lifted.iter().any(|message| message.0.id == good));
    let warnings = capture.output();
    let warnings = warnings
        .lines()
        .filter(|line| line.contains("skipping stored message"))
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1, "expected one warning: {warnings:?}");
    let warning: serde_json::Value = serde_json::from_str(warnings[0])?;
    assert_eq!(warning["message_id"], bad.checked()?);
    assert!(
        warning["error"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty())
    );
    assert!(!warnings[0].contains("sensitive-history-content"));
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_skips_bad_reaction_and_warns_without_content() {
    use crate::{Reaction, ReactionAction, ReactionSchema};
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};
    use xmtp_logging::{Level, test_logging::LogCapture};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent = group.send_text("parent row".into(), None).await?;
    let reaction = client
        .conversations()
        .react_to_message(
            parent.clone(),
            Reaction {
                content: "sensitive-reaction-content".into(),
                action: ReactionAction::Added,
                schema: ReactionSchema::Unicode,
            },
            None,
        )
        .await?;
    let reaction_bytes = reaction.to_bytes()?;
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(&reaction_bytes)))
            .set(dsl::sender_inbox_id.eq(""))
            .execute(conn)
    })?;

    let history = group.messages(None).await?;
    let parent_message = history
        .iter()
        .find(|message| message.0.id == parent)
        .expect("parent in history");
    assert!(parent_message.0.reactions.is_empty());

    let enriched = group
        .inner
        .find_messages_v2_with_stored(&MsgQueryArgs::default())?;
    let capture = LogCapture::new(Level::Warn);
    let lifted = tracing::dispatcher::with_default(&capture.dispatch(), || {
        crate::conversation::lift_history_messages(enriched, client.client_key())
    });
    let parent_message = lifted
        .iter()
        .find(|message| message.0.id == parent)
        .expect("lifted parent");
    assert!(parent_message.0.reactions.is_empty());
    let warnings = capture.output();
    let warnings = warnings
        .lines()
        .filter(|line| line.contains("skipping stored reaction"))
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1, "expected one warning: {warnings:?}");
    let warning: serde_json::Value = serde_json::from_str(warnings[0])?;
    assert_eq!(warning["reaction_id"], reaction.checked()?);
    assert!(
        warning["error"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty())
    );
    assert!(!warnings[0].contains("sensitive-reaction-content"));
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn reply_omits_bad_parent_and_warns_without_content() {
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};
    use xmtp_logging::{Level, test_logging::LogCapture};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent = group
        .send_text("sensitive-parent-content".into(), None)
        .await?;
    let reply = client
        .conversations()
        .reply_to_message(parent.clone(), crate::encode_text("reply".into())?, None)
        .await?;
    let parent_bytes = parent.to_bytes()?;
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(&parent_bytes)))
            .set(dsl::sender_inbox_id.eq(""))
            .execute(conn)
    })?;

    let by_id = client
        .conversations()
        .get_message_by_id(reply.clone())
        .await?
        .expect("reply by ID");
    assert!(by_id.0.in_reply_to.is_none());
    let history = group.messages(None).await?;
    let history_reply = history
        .iter()
        .find(|message| message.0.id == reply)
        .expect("reply in history");
    assert!(history_reply.0.in_reply_to.is_none());

    let reply_bytes = reply.to_bytes()?;
    let enriched = group
        .inner
        .find_messages_v2_with_stored(&MsgQueryArgs::default())?
        .into_iter()
        .filter(|message| message.stored.id == reply_bytes)
        .collect();
    let capture = LogCapture::new(Level::Warn);
    let lifted = tracing::dispatcher::with_default(&capture.dispatch(), || {
        crate::conversation::lift_history_messages(enriched, client.client_key())
    });
    assert_eq!(lifted.len(), 1);
    assert!(lifted[0].0.in_reply_to.is_none());
    let warnings = capture.output();
    let warnings = warnings
        .lines()
        .filter(|line| line.contains("omitting reply parent"))
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1, "expected one warning: {warnings:?}");
    let warning: serde_json::Value = serde_json::from_str(warnings[0])?;
    assert_eq!(warning["parent_id"], parent.checked()?);
    assert!(
        warning["error"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty())
    );
    assert!(!warnings[0].contains("sensitive-parent-content"));
    client.end().await?;
}
