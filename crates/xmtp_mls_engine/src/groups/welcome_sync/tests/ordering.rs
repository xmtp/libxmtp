//! Welcome sync tests: independent joins, terminal progress and later completions.

use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn blocked_welcome_does_not_hold_an_independent_join() {
    use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
    use xmtp_db::ConnectionExt;
    use xmtp_db::schema::incoming_envelopes;
    use xmtp_proto::backend_v1::{client_envelope::Payload, welcome_message::Version};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let first_group = alix.create_group(None, None)?;
    first_group.invite(&bo).await?;
    let second_group = alix.create_group(None, None)?;
    second_group.invite(&bo).await?;
    let welcomes = bo
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await?;
    let first = welcomes.first()?.cursor;
    let last = welcomes.last()?.cursor;
    pending_welcome_for_test(&bo.context, welcomes.last()?).await?;
    let service = WelcomeService::new(bo.context.clone());
    let db = bo.context.db();
    let pending = db.pending_envelope(&service.topic(), first)?.unwrap();
    let mut wire = xmtp_proto::backend_v1::ServerEnvelope::decode(pending.envelope.as_slice())?;
    let Payload::WelcomeMessage(welcome) = wire.envelope.as_mut()?.payload.as_mut()? else {
        panic!("expected Welcome payload");
    };
    match welcome.version.as_mut()? {
        Version::V1(message) => message.wrapper_algorithm = i32::MAX,
        Version::WelcomePointer(message) => message.wrapper_algorithm = i32::MAX,
    }
    let envelope = wire.encode_to_vec();
    db.raw_query(|conn| {
        diesel::update(incoming_envelopes::table.find((
            bo.context.installation_id().to_vec(),
            EntityKind::Welcome,
            first.0 as i64,
        )))
        .set((
            incoming_envelopes::envelope_bytes.eq(envelope.len() as i64),
            incoming_envelopes::envelope.eq(&envelope),
        ))
        .execute(conn)
    })?;

    let outcomes = service.process_pending_welcomes_once(&[])?;
    assert!(outcomes.iter().any(|outcome| matches!(outcome,
        WelcomeHeadOutcome::Waiting { cursor, blocked: true, .. } if *cursor == first)));
    assert!(outcomes.iter().any(|outcome| matches!(outcome,
        WelcomeHeadOutcome::Need { cursor, .. } | WelcomeHeadOutcome::Progress { cursor, .. } if *cursor == last)));
    if db.find_group(&second_group.group_id)?.is_none() {
        assert!(matches!(
            service.resolve_pending_welcome(last).await?,
            WelcomeHeadOutcome::Progress {
                result: Ok(Some(_)),
                ..
            }
        ));
    }
    assert!(db.find_group(&first_group.group_id)?.is_none());
    assert!(db.find_group(&second_group.group_id)?.is_some());
    let first_pending = db.pending_envelope(&service.topic(), first)?.unwrap();
    assert!(first_pending.blocked);
    assert_eq!(
        first_pending.error_code.as_deref(),
        Some("unsupported_welcome")
    );
    assert!(db.pending_envelope(&service.topic(), last)?.is_none());
    let progress = db.topic_progress(&service.topic())?;
    assert!(progress.processed < first);
    assert_eq!(progress.received, last);
}

impl ValidateGroupMembership for RejectMembership {
    async fn check_initial_membership(
        &self,
        _welcome: &WelcomeMembership,
    ) -> Result<(), GroupError> {
        if self.retryable {
            Err(GroupError::LockUnavailable)
        } else {
            Err(GroupError::NoPSKSupport)
        }
    }
}

#[rstest]
#[case::terminal(false)]
#[case::retry(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn welcome_rejection_commits_only_terminal_progress(#[case] retryable: bool) {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None).unwrap();
    group.invite(&bo).await.unwrap();
    let welcome = bo
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await
        .unwrap()
        .pop()
        .unwrap();
    pending_welcome_for_test(&bo.context, &welcome)
        .await
        .unwrap();
    let service = WelcomeService::new(bo.context.clone());
    let result = service
        .process_new_welcome(&welcome, RejectMembership { retryable })
        .await;
    assert!(result.is_err());
    assert!(
        bo.context
            .db()
            .find_group(&group.group_id)
            .unwrap()
            .is_none()
    );
    let expected = if !retryable {
        welcome.cursor
    } else {
        Cursor(0)
    };
    assert_eq!(
        bo.context
            .db()
            .get_last_cursor(bo.context.installation_id(), EntityKind::Welcome)
            .unwrap(),
        expected
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn later_welcome_completion_keeps_earlier_retry_pending() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let first_group = alix.create_group(None, None)?;
    first_group.invite(&bo).await?;
    let second_group = alix.create_group(None, None)?;
    second_group.invite(&bo).await?;
    let welcomes = bo
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await?;
    let first = welcomes.first()?.cursor;
    let last = welcomes.last()?.cursor;
    pending_welcome_for_test(&bo.context, welcomes.last()?).await?;
    let service = WelcomeService::new(bo.context.clone());
    for welcome in welcomes {
        let result = service
            .process_new_welcome(
                &welcome,
                RejectMembership {
                    retryable: welcome.cursor == first,
                },
            )
            .await;
        assert!(result.is_err());
    }
    let db = bo.context.db();
    assert!(db.pending_envelope(&service.topic(), first)?.is_some());
    assert!(db.pending_envelope(&service.topic(), last)?.is_none());
    let progress = db.topic_progress(&service.topic())?;
    assert!(progress.processed < first);
    assert_eq!(progress.received, last);
}
