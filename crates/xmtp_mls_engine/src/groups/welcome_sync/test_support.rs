//! Test fixtures for pending welcomes: the stored envelope of a welcome, and a held re-add.

use super::*;

/// The pending envelope that stores `welcome` for this installation. It
/// receives Welcome pages until the envelope is stored, and fails with
/// `WelcomeAlreadyProcessed` once the pages end without it.
#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub async fn pending_welcome_for_test(
    context: &impl XmtpSharedContext,
    welcome: &xmtp_proto::types::WelcomeMessage,
) -> Result<StoredIncomingEnvelope, GroupError> {
    let topic = StreamTopic {
        entity_id: context.installation_id().to_vec(),
        kind: NetworkEntityKind::Welcome,
    };
    let store = MlsStore::new(context.clone());
    loop {
        if let Some(pending) = context.db().pending_envelope(&topic, welcome.cursor)? {
            return Ok(pending);
        }
        let page = store
            .receive_topics_once(
                &[Topic::new_welcome_message(context.installation_id())],
                context
                    .incoming_runtime()
                    .policy()
                    .incoming_limits(NetworkEntityKind::Welcome),
            )
            .await?;
        if !page.has_more {
            return context
                .db()
                .pending_envelope(&topic, welcome.cursor)?
                .ok_or_else(|| ProcessIntentError::WelcomeAlreadyProcessed(welcome.cursor).into());
        }
    }
}

/// Alix's group, from which Alix removed and then re-added Bo. Bo still holds
/// the group as active: it received the group topic only through its removal,
/// because the secrets of its old membership cannot process the re-add commit,
/// and it processed none of it. The re-add Welcome is pending.
#[cfg(test)]
pub(crate) struct HeldReadd<C> {
    pub(crate) alix_group: MlsGroup<C>,
    pub(crate) bo_group: MlsGroup<C>,
    pub(crate) welcome: xmtp_proto::types::WelcomeMessage,
    /// Bo's receipt of the group topic through its removal.
    pub(crate) removal: xmtp_db::incoming_envelope::AdmissionResult,
}

#[cfg(test)]
pub(crate) async fn held_readd<P: crate::groups::test_support::EnginePeer>(
    alix: &P,
    bo: &P,
) -> HeldReadd<P::Context> {
    use xmtp_mls_core::context::CoreContext;
    let alix_group = alix.create_group(None, None).unwrap();
    alix_group
        .add_members(&[bo.context().inbox_id()])
        .await
        .unwrap();
    let bo_group = bo.sync_welcomes().await.unwrap().pop().unwrap();
    alix_group
        .remove_members(&[bo.context().inbox_id()])
        .await
        .unwrap();
    let page = MlsStore::new(bo.context().clone())
        .receive_topics_once(
            &[Topic::new_group_message(bo_group.group_id)],
            bo.context()
                .incoming_runtime()
                .policy()
                .incoming_limits(NetworkEntityKind::Group),
        )
        .await
        .unwrap();
    assert!(!page.has_more);
    let removal = page.admissions[0].1;
    assert!(removal.inserted > 0);
    alix_group
        .add_members(&[bo.context().inbox_id()])
        .await
        .unwrap();
    let welcome = bo
        .context()
        .api()
        .query_welcome_messages(bo.context().installation_id())
        .await
        .unwrap()
        .into_iter()
        .max_by_key(|welcome| welcome.cursor)
        .unwrap();
    pending_welcome_for_test(bo.context(), &welcome)
        .await
        .unwrap();
    HeldReadd {
        alix_group,
        bo_group,
        welcome,
        removal,
    }
}
