//! Client group tests: dm blocks.

use super::*;

/// A DM's consent is its peer's inbox record, so one decision covers every
/// group of the pair, through any group id or the inbox, and every reader
/// sees it: listing, the conversation list, push, and commit-log selection.
/// A group with the peer keeps its own record.
// verifies: CONS-052, CONS-060, CONS-059
#[xmtp_common::test(unwrap_try = true)]
async fn dm_block_covers_every_group_with_the_peer() {
    use xmtp_db::consent_record::{ConsentKey, ConsentState, ConsentType};
    use xmtp_db::group::GroupQueryArgs;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let peer = ConsentKey::inbox(bo.inbox_id());

    let g1 = alix
        .create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
        .await?;
    let g2 = alix
        .create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
        .await?;
    assert_ne!(g1.group_id, g2.group_id);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    let commit_log_ids = || -> Result<Vec<_>, xmtp_db::ConnectionError> {
        Ok(alix
            .db()
            .get_conversation_ids_for_remote_log_publish()?
            .into_iter()
            .map(|row| row.id)
            .collect())
    };
    let selected = commit_log_ids()?;
    assert!(selected.contains(&g1.group_id) && selected.contains(&g2.group_id));
    // Both read allowed from creating, through the peer's record.
    assert_eq!(g1.consent_state()?, ConsentState::Allowed);
    assert_eq!(g2.consent_state()?, ConsentState::Allowed);
    assert!(
        alix.db()
            .consent_row(&ConsentKey::group(g1.group_id.as_ref()))?
            .is_none()
    );

    let generation = || {
        alix.db().raw_query(|c| {
            xmtp_db::diesel::sql_query(
                "SELECT generation AS value FROM group_set_generation WHERE id = 1",
            )
            .get_result::<Generation>(c)
        })
    };
    #[derive(xmtp_db::diesel::QueryableByName)]
    struct Generation {
        #[diesel(sql_type = xmtp_db::diesel::sql_types::BigInt)]
        value: i64,
    }
    let before = generation()?.value;
    // Deny through one group id; every group of the pair is denied.
    g2.update_consent_state(ConsentState::Denied)?;
    assert!(
        generation()?.value > before,
        "inbox consent moves the group set"
    );
    for group in [&g1, &g2] {
        assert_eq!(group.consent_state()?, ConsentState::Denied);
    }
    assert_eq!(
        alix.get_consent_state(ConsentType::InboxId, bo.inbox_id().to_string())
            .await?,
        ConsentState::Denied
    );

    // The default listing (Allowed + Unknown) excludes every group of the
    // pair, and keeps a group with the peer, which has its own record.
    let listed = alix.find_groups(GroupQueryArgs::default())?;
    assert!(listed.iter().all(|g| g.dm_id.is_none()));
    assert!(listed.iter().any(|g| g.group_id == group.group_id));
    let all = alix.find_groups(GroupQueryArgs {
        consent_states: Some(vec![ConsentState::Denied]),
        ..Default::default()
    })?;
    assert!(
        all.iter().any(|g| g.group_id == g1.group_id)
            || all.iter().any(|g| g.group_id == g2.group_id)
    );
    let list = alix.list_conversations(GroupQueryArgs::default())?;
    assert!(list.iter().all(|c| c.group.dm_id.is_none()));
    // Push and commit-log publication read the same record.
    assert!(!g1.notifications_enabled()?);
    let selected = commit_log_ids()?;
    assert!(!selected.contains(&g1.group_id) && !selected.contains(&g2.group_id));

    // A group id the client does not hold is its own key, which holds nothing.
    let g3 = xmtp_proto::types::GroupId::from([0x5b; 32]);
    assert_eq!(
        xmtp_db::consent_record::conversation_consent(&alix.db(), alix.inbox_id(), &g3)?,
        ConsentState::Unknown
    );
    // Allow through the inbox directly, then both DMs are allowed again.
    alix.set_consent_states(&[crate::client::ConsentAct {
        entity_type: ConsentType::InboxId,
        entity: bo.inbox_id().to_string(),
        state: ConsentState::Allowed,
    }])
    .await?;
    for group in [&g1, &g2] {
        assert_eq!(group.consent_state()?, ConsentState::Allowed);
    }
    assert_eq!(
        alix.db().consent_row(&peer)?.unwrap().state,
        ConsentState::Allowed
    );
}

/// An act naming a conversation the client does not hold fails and stores
/// nothing, also for the other acts of its call. An act on an inbox the
/// client has never seen is stored and covers a DM that arrives later. A read
/// of the unheld id still answers unknown.
// verifies: CONS-052
#[xmtp_common::test(unwrap_try = true)]
async fn act_on_unheld_conversation_is_refused() {
    use crate::client::ConsentAct;
    use xmtp_db::consent_record::{ConsentActError, ConsentKey, ConsentState, ConsentType};

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let unheld = hex::encode([0x6c; 32]);
    let refused = ConsentAct::new(
        ConsentType::ConversationId,
        ConsentState::Denied,
        unheld.clone(),
    );
    let inbox = ConsentAct::new(
        ConsentType::InboxId,
        ConsentState::Denied,
        bo.inbox_id().to_string(),
    );
    let before = alix.db().consent_records()?.len();
    for acts in [vec![refused.clone()], vec![inbox.clone(), refused]] {
        let result = alix.set_consent_states(&acts).await;
        assert!(
            matches!(
                result,
                Err(crate::client::ClientError::ConsentAct(ConsentActError::ConversationNotHeld(ref id))) if *id == unheld
            ),
            "{result:?}"
        );
        assert_eq!(
            alix.db().consent_records()?.len(),
            before,
            "nothing is stored"
        );
    }
    assert_eq!(
        alix.get_consent_state(ConsentType::ConversationId, unheld)
            .await?,
        ConsentState::Unknown
    );
    // An inbox the client has never seen can be set, and a DM that arrives
    // later reads it.
    alix.set_consent_states(&[inbox]).await?;
    assert!(
        alix.db()
            .consent_row(&ConsentKey::inbox(bo.inbox_id()))?
            .is_some()
    );
    let dm = bo.find_or_create_dm(alix.inbox_id(), None).await?;
    dm.update_installations().await?;
    alix.sync_welcomes().await?;
    assert_eq!(
        alix.group(&dm.group_id)?.consent_state()?,
        ConsentState::Denied
    );
}
