//! Signed group messages. Every saved envelope carries the group signing
//! key's signature, and a group that cannot sign fails its intents instead of
//! blocking its queue.

use super::faults::{FaultyApi, faulty_group, group_message};
use super::*;
use xmtp_db::group_message::DeliveryStatus;
use xmtp_mls_common::{
    app_data::{component_id::ComponentId, group_root::GroupRootError},
    group_keys::{group_message_tbs, verify_publish_signature},
};
use xmtp_proto::backend_v1 as wire;

/// The saved envelopes of an intent's attempt, read back from the database.
fn saved_envelopes<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    intent_id: i32,
) -> Vec<wire::ClientEnvelope> {
    let saved = group
        .context
        .db()
        .prepared_envelopes(intent_id)
        .unwrap()
        .expect("a prepared intent has saved bytes");
    PreparedAttempt::decode(&saved)
        .unwrap()
        .envelopes
        .iter()
        .map(|bytes| wire::ClientEnvelope::decode(bytes.as_slice()).unwrap())
        .collect()
}

/// The envelope is a group message whose signature passes the backend's
/// publish-signature check over the `GroupMessageTBS` rebuilt from its own
/// fields, under the group id.
fn assert_signed(group_id: &GroupId, envelope: &wire::ClientEnvelope) {
    let Some(Payload::GroupMessage(message)) = &envelope.payload else {
        panic!("expected a group message, got {envelope:?}");
    };
    let group_id: [u8; 32] = group_id
        .as_slice()
        .try_into()
        .expect("a keyed group id is 32 bytes");
    let tbs = group_message_tbs(
        &group_id,
        &message.data,
        &message.sender_hmac,
        message.should_push,
    )
    .unwrap();
    verify_publish_signature(&group_id, &tbs, &message.signature)
        .expect("a saved group message is signed by the group signing key");
}

/// Remove `GROUP_ROOT_KEY` from the group context OpenMLS stores, as if the
/// group never held a root. The component is immutable, so no commit can do
/// this; the test rewrites the stored context instead.
fn remove_group_root<C: XmtpSharedContext>(group: &MlsGroup<C>) {
    use openmls::extensions::{AppDataDictionaryExtension, Extension, Extensions};
    use openmls::group::GroupContext;
    use openmls::prelude::tls_codec::{Deserialize as _, Serialize as _};
    use openmls_traits::storage::StorageProvider as _;

    crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, storage| {
            let context = mls_group.export_group_context();
            let extensions = context
                .extensions()
                .iter()
                .cloned()
                .map(|extension| match extension {
                    Extension::AppDataDictionary(dictionary) => {
                        let mut dictionary = dictionary.dictionary().clone();
                        assert!(
                            dictionary
                                .remove(&ComponentId::GROUP_ROOT_KEY.as_u16())
                                .is_some(),
                            "a keyed group holds a root"
                        );
                        Extension::AppDataDictionary(AppDataDictionaryExtension::new(dictionary))
                    }
                    other => other,
                })
                .collect();
            let extensions = Extensions::<GroupContext>::from_vec(extensions).unwrap();
            // The extensions are the last field of the TLS-encoded context.
            let mut bytes = context.tls_serialize_detached().unwrap();
            let old = context.extensions().tls_serialize_detached().unwrap();
            assert!(bytes.ends_with(&old));
            bytes.truncate(bytes.len() - old.len());
            bytes.extend(extensions.tls_serialize_detached().unwrap());
            let rewritten = GroupContext::tls_deserialize_exact(bytes).unwrap();
            storage
                .write_context(mls_group.group_id(), &rewritten)
                .unwrap();
            Ok::<_, GroupError>(Continue(()))
        })
    })
    .unwrap();
}

/// Preparation signs every group-message envelope before it saves the
/// attempt, so the saved bytes are the signed bytes. A restarted client
/// publishes exactly those bytes, and the intent completes.
// verifies: SEND-006, GMOD-041
#[rstest::rstest]
#[case::message(IntentKind::SendMessage)]
#[case::commit(IntentKind::KeyUpdate)]
#[xmtp_common::test(unwrap_try = true)]
async fn saved_group_messages_carry_a_valid_signature(
    #[case] kind: IntentKind,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let message_id = match kind {
        IntentKind::SendMessage => {
            Some(group.send_message_optimistic(b"signed at preparation", Default::default())?)
        }
        _ => {
            QueueIntent::key_update().queue(&group)?;
            None
        }
    };
    let (intent, _) = prepare_kind(&group, kind).await?;

    let saved = saved_envelopes(&group, intent.id);
    assert!(!saved.is_empty());
    for envelope in &saved {
        assert_signed(&group.group_id, envelope);
    }

    // Publish from the saved bytes alone, in a restarted client.
    let api = FaultyApi::new(&alix, group_message, []);
    let restarted = faulty_group(&alix, &group.group_id, api.clone()).await;
    restarted.publish_intents().await?;
    for envelope in &saved {
        assert_eq!(api.sends_of(envelope), 1, "the saved bytes are sent once");
    }
    restarted.sync_until_intent_resolved(intent.id).await?;
    let resolved: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert!(
        !matches!(
            resolved.state,
            IntentState::ToPublish | IntentState::Published | IntentState::Error
        ),
        "{:?}",
        resolved.state
    );
    if let Some(message_id) = message_id {
        let message: StoredGroupMessage = group.context.db().fetch(&message_id)?.unwrap();
        assert_eq!(message.delivery_status, DeliveryStatus::Published);
    }
    Ok(())
}

/// A group whose stored root is gone cannot sign. One `publish_intents` call
/// fails both of its queued sends, so neither is left to block the queue.
// verifies: SEND-009
#[xmtp_common::test(unwrap_try = true)]
async fn a_group_without_its_root_fails_every_queued_intent() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let messages = [
        group.send_message_optimistic(b"first", Default::default())?,
        group.send_message_optimistic(b"second", Default::default())?,
    ];
    remove_group_root(&group);

    let error = group.publish_intents().await.unwrap_err();
    assert!(
        matches!(
            error,
            GroupError::OutgoingPreparation(OutgoingPreparationError::GroupKeys(
                GroupRootError::Missing
            ))
        ),
        "{error:?}"
    );
    let intents = group.context.db().find_group_intents(
        group.group_id,
        None,
        Some(vec![IntentKind::SendMessage]),
    )?;
    assert_eq!(intents.len(), 2);
    for intent in intents {
        assert_eq!(intent.state, IntentState::Error);
        assert!(group.context.db().prepared_envelopes(intent.id)?.is_none());
    }
    for message_id in messages {
        let message: StoredGroupMessage = group.context.db().fetch(&message_id)?.unwrap();
        assert_eq!(message.delivery_status, DeliveryStatus::Failed);
    }
}
